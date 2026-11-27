use super::{Address, Label, Register, Width};
use crate::Fault;
use simplex_object::{Fixup, Relocation};

pub(crate) struct Patch {
    pub(crate) offset: usize,
    pub(crate) target: Label,
}

#[derive(Default)]
pub struct Encoder {
    pub bytes: Vec<u8>,
    pub(crate) marks: Vec<Option<usize>>,
    pub(crate) patches: Vec<Patch>,
    pub relocations: Vec<Relocation>,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh(&mut self) -> Label {
        let index = self.marks.len() as u32;
        self.marks.push(None);
        Label(index)
    }

    pub fn bind(&mut self, label: Label) {
        self.marks[label.0 as usize] = Some(self.bytes.len());
    }

    pub(crate) fn rex(&mut self, wide: bool, reg: u8, rm: u8) {
        self.tag(wide, reg, rm, false);
    }

    pub(crate) fn tag(&mut self, wide: bool, reg: u8, rm: u8, force: bool) {
        let bits = (u8::from(wide) << 3) | (u8::from(reg >= 8) << 2) | u8::from(rm >= 8);
        if bits != 0 || force {
            self.bytes.push(0x40 | bits);
        }
    }

    pub(crate) const fn pair(mode: u8, reg: u8, rm: u8) -> u8 {
        ((mode & 3) << 6) | ((reg & 7) << 3) | (rm & 7)
    }

    pub(crate) fn memory(&mut self, reg: u8, addr: Address) {
        let rm = addr.base.code();
        let low = rm & 7;
        let off = addr.offset;
        match off {
            0 if low != 5 => {
                self.bytes.push(Self::pair(0, reg, rm));
                if low == 4 {
                    self.bytes.push(0x24);
                }
            }
            -128..=127 => {
                self.bytes.push(Self::pair(1, reg, rm));
                if low == 4 {
                    self.bytes.push(0x24);
                }
                self.bytes.push(off as i8 as u8);
            }
            _ => {
                self.bytes.push(Self::pair(2, reg, rm));
                if low == 4 {
                    self.bytes.push(0x24);
                }
                self.bytes.extend_from_slice(&off.to_le_bytes());
            }
        }
    }

    pub fn copy(&mut self, dst: Register, src: Register) {
        if dst != src {
            self.rex(true, src.code(), dst.code());
            self.bytes
                .extend_from_slice(&[0x89, Self::pair(3, src.code(), dst.code())]);
        }
    }

    pub fn constant(&mut self, dst: Register, value: i64) {
        let code = dst.code();
        match value {
            0 => {
                self.rex(false, code, code);
                self.bytes
                    .extend_from_slice(&[0x31, Self::pair(3, code, code)]);
            }
            v if v > 0 && v <= i64::from(u32::MAX) => {
                self.rex(false, 0, code);
                self.bytes.push(0xB8 + (code & 7));
                self.bytes.extend_from_slice(&(v as u32).to_le_bytes());
            }
            v if v >= i64::from(i32::MIN) && v < 0 => {
                self.rex(true, 0, code);
                self.bytes
                    .extend_from_slice(&[0xC7, Self::pair(3, 0, code)]);
                self.bytes.extend_from_slice(&(v as i32).to_le_bytes());
            }
            v => {
                self.rex(true, 0, code);
                self.bytes.push(0xB8 + (code & 7));
                self.bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
    }

    pub fn load(&mut self, dst: Register, addr: Address, width: Width) {
        let d = dst.code();
        let b = addr.base.code();
        match width {
            Width::Quad => {
                self.rex(true, d, b);
                self.bytes.push(0x8B);
            }
            Width::Double => {
                self.rex(false, d, b);
                self.bytes.push(0x8B);
            }
            Width::Word => {
                self.rex(false, d, b);
                self.bytes.extend_from_slice(&[0x0F, 0xB7]);
            }
            Width::Byte => {
                self.rex(false, d, b);
                self.bytes.extend_from_slice(&[0x0F, 0xB6]);
            }
        }
        self.memory(d, addr);
    }

    pub fn signed(&mut self, dst: Register, addr: Address, width: Width) {
        let d = dst.code();
        let b = addr.base.code();
        self.rex(true, d, b);
        match width {
            Width::Quad => self.bytes.push(0x8B),
            Width::Double => self.bytes.push(0x63),
            Width::Word => self.bytes.extend_from_slice(&[0x0F, 0xBF]),
            Width::Byte => self.bytes.extend_from_slice(&[0x0F, 0xBE]),
        }
        self.memory(d, addr);
    }

    pub fn store(&mut self, addr: Address, src: Register, width: Width) {
        let s = src.code();
        let b = addr.base.code();
        match width {
            Width::Quad => {
                self.rex(true, s, b);
                self.bytes.push(0x89);
            }
            Width::Double => {
                self.rex(false, s, b);
                self.bytes.push(0x89);
            }
            Width::Word => {
                self.bytes.push(0x66);
                self.rex(false, s, b);
                self.bytes.push(0x89);
            }
            Width::Byte => {
                self.tag(false, s, b, s >= 4);
                self.bytes.push(0x88);
            }
        }
        self.memory(s, addr);
    }

    pub fn extend(&mut self, dst: Register, src: Register, width: Width) {
        let d = dst.code();
        let s = src.code();
        match width {
            Width::Quad => self.copy(dst, src),
            Width::Double => {
                self.rex(false, s, d);
                self.bytes.extend_from_slice(&[0x89, Self::pair(3, s, d)]);
            }
            Width::Word => {
                self.rex(false, d, s);
                self.bytes
                    .extend_from_slice(&[0x0F, 0xB7, Self::pair(3, d, s)]);
            }
            Width::Byte => {
                self.tag(false, d, s, s >= 4);
                self.bytes
                    .extend_from_slice(&[0x0F, 0xB6, Self::pair(3, d, s)]);
            }
        }
    }

    pub fn widen(&mut self, dst: Register, src: Register, width: Width) {
        let d = dst.code();
        let s = src.code();
        match width {
            Width::Quad => self.copy(dst, src),
            Width::Double => {
                self.rex(true, d, s);
                self.bytes.extend_from_slice(&[0x63, Self::pair(3, d, s)]);
            }
            Width::Word => {
                self.rex(true, d, s);
                self.bytes
                    .extend_from_slice(&[0x0F, 0xBF, Self::pair(3, d, s)]);
            }
            Width::Byte => {
                self.tag(true, d, s, s >= 4);
                self.bytes
                    .extend_from_slice(&[0x0F, 0xBE, Self::pair(3, d, s)]);
            }
        }
    }

    pub fn address(&mut self, dst: Register, addr: Address) {
        self.rex(true, dst.code(), addr.base.code());
        self.bytes.push(0x8D);
        self.memory(dst.code(), addr);
    }

    pub fn symbol(&mut self, dst: Register, name: &str) {
        self.rex(true, dst.code(), 0);
        self.bytes
            .extend_from_slice(&[0x8D, Self::pair(0, dst.code(), 5)]);
        self.relocate(name, Fixup::Relative);
    }

    pub fn global(&mut self, dst: Register, name: &str) {
        self.rex(true, dst.code(), 0);
        self.bytes
            .extend_from_slice(&[0x8B, Self::pair(0, dst.code(), 5)]);
        self.relocate(name, Fixup::Got);
    }

    pub fn call(&mut self, symbol: &str) {
        self.bytes.push(0xE8);
        self.relocate(symbol, Fixup::Call);
    }

    fn relocate(&mut self, name: &str, kind: Fixup) {
        let offset = self.bytes.len() as u64;
        self.relocations.push(Relocation {
            offset,
            symbol: name.to_owned(),
            kind,
            addend: -4,
        });
        self.bytes.extend_from_slice(&[0; 4]);
    }

    pub fn indirect(&mut self, target: Register) {
        self.rex(false, 0, target.code());
        self.bytes
            .extend_from_slice(&[0xFF, Self::pair(3, 2, target.code())]);
    }

    pub fn push(&mut self, reg: Register) {
        let code = reg.code();
        if code >= 8 {
            self.bytes.push(0x41);
        }
        self.bytes.push(0x50 + (code & 7));
    }

    pub fn pop(&mut self, reg: Register) {
        let code = reg.code();
        if code >= 8 {
            self.bytes.push(0x41);
        }
        self.bytes.push(0x58 + (code & 7));
    }

    pub fn leave(&mut self) {
        self.bytes.push(0xC3);
    }

    pub fn finish(mut self) -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
        for item in &self.patches {
            let Some(dest) = self.marks.get(item.target.0 as usize).copied().flatten() else {
                return Err(Fault::Unsupported("unbound label".to_owned()));
            };
            let delta = dest as i64 - (item.offset as i64 + 4);
            let bytes = (delta as i32).to_le_bytes();
            self.bytes[item.offset..item.offset + 4].copy_from_slice(&bytes);
        }
        Ok((self.bytes, self.relocations))
    }
}
