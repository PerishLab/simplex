use super::{Condition, Encoder, Label, Patch, Shift, Width};
use crate::{Fault, Register};
use simplex_elf::{Kind, Relocation};

impl Encoder {
    pub fn symbol(&mut self, dst: Register, name: &str) {
        self.rex(true, dst.code(), 0);
        self.bytes
            .extend_from_slice(&[0x8D, Self::pair(0, dst.code(), 5)]);
        self.relocate(name, Kind::Relative32);
    }

    pub fn global(&mut self, dst: Register, name: &str) {
        self.rex(true, dst.code(), 0);
        self.bytes
            .extend_from_slice(&[0x8B, Self::pair(0, dst.code(), 5)]);
        self.relocate(name, Kind::Global32);
    }

    fn relocate(&mut self, name: &str, kind: Kind) {
        let offset = self.bytes.len() as u64;
        self.relocations.push(Relocation {
            offset,
            symbol: name.to_owned(),
            kind,
            addend: -4,
        });
        self.bytes.extend_from_slice(&[0; 4]);
    }

    pub fn binary(&mut self, code: u8, dst: Register, rhs: Register) {
        self.rex(true, rhs.code(), dst.code());
        self.bytes
            .extend_from_slice(&[code, Self::pair(3, rhs.code(), dst.code())]);
    }

    pub fn add(&mut self, dst: Register, rhs: Register) {
        self.binary(0x01, dst, rhs);
    }

    pub fn sub(&mut self, dst: Register, rhs: Register) {
        self.binary(0x29, dst, rhs);
    }

    pub fn bitand(&mut self, dst: Register, rhs: Register) {
        self.binary(0x21, dst, rhs);
    }

    pub fn bitor(&mut self, dst: Register, rhs: Register) {
        self.binary(0x09, dst, rhs);
    }

    pub fn bitxor(&mut self, dst: Register, rhs: Register) {
        self.binary(0x31, dst, rhs);
    }

    pub fn multiply(&mut self, dst: Register, rhs: Register) {
        self.rex(true, dst.code(), rhs.code());
        self.bytes
            .extend_from_slice(&[0x0F, 0xAF, Self::pair(3, dst.code(), rhs.code())]);
    }

    pub fn divide(&mut self, signed: bool) {
        match signed {
            true => self
                .bytes
                .extend_from_slice(&[0x48, 0x99, 0x48, 0xF7, 0xF9]),
            false => self
                .bytes
                .extend_from_slice(&[0x31, 0xD2, 0x48, 0xF7, 0xF1]),
        }
    }

    pub fn shift(&mut self, kind: Shift, dst: Register) {
        self.rex(true, 0, dst.code());
        self.bytes
            .extend_from_slice(&[0xD3, Self::pair(3, kind as u8, dst.code())]);
    }

    pub fn negate(&mut self, dst: Register) {
        self.rex(true, 0, dst.code());
        self.bytes
            .extend_from_slice(&[0xF7, Self::pair(3, 3, dst.code())]);
    }

    pub fn invert(&mut self, dst: Register) {
        self.rex(true, 0, dst.code());
        self.bytes
            .extend_from_slice(&[0xF7, Self::pair(3, 2, dst.code())]);
    }

    pub fn adjust(&mut self, dst: Register, delta: i32) {
        match delta {
            0 => {}
            -128..=127 => {
                self.rex(true, 0, dst.code());
                self.bytes.extend_from_slice(&[
                    0x83,
                    Self::pair(3, 0, dst.code()),
                    delta as i8 as u8,
                ]);
            }
            d if d > 0 => {
                self.rex(true, 0, dst.code());
                self.bytes
                    .extend_from_slice(&[0x81, Self::pair(3, 0, dst.code())]);
                self.bytes.extend_from_slice(&d.to_le_bytes());
            }
            d => {
                self.rex(true, 0, dst.code());
                self.bytes
                    .extend_from_slice(&[0x81, Self::pair(3, 5, dst.code())]);
                self.bytes
                    .extend_from_slice(&d.wrapping_neg().to_le_bytes());
            }
        }
    }

    pub fn align(&mut self) {
        self.bytes.extend_from_slice(&[0x48, 0x83, 0xE4, 0xF0]);
    }

    pub fn compare(&mut self, lhs: Register, rhs: Register) {
        self.rex(true, rhs.code(), lhs.code());
        self.bytes
            .extend_from_slice(&[0x39, Self::pair(3, rhs.code(), lhs.code())]);
    }

    pub fn test(&mut self, reg: Register) {
        self.rex(true, reg.code(), reg.code());
        self.bytes
            .extend_from_slice(&[0x85, Self::pair(3, reg.code(), reg.code())]);
    }

    pub fn flag(&mut self, dst: Register, cond: Condition) {
        let d = dst.code();
        match d {
            8.. => self.bytes.push(0x41),
            4..=7 => self.bytes.push(0x40),
            _ => {}
        }
        self.bytes
            .extend_from_slice(&[0x0F, 0x90 + cond as u8, Self::pair(3, 0, d)]);
        self.extend(dst, dst, Width::Byte);
    }

    pub fn jump(&mut self, target: Label) {
        self.bytes.push(0xE9);
        self.patch(target);
    }

    pub fn branch(&mut self, cond: Condition, target: Label) {
        self.bytes.extend_from_slice(&[0x0F, 0x80 + cond as u8]);
        self.patch(target);
    }

    fn patch(&mut self, target: Label) {
        let offset = self.bytes.len();
        self.patches.push(Patch { offset, target });
        self.bytes.extend_from_slice(&[0; 4]);
    }

    pub fn call(&mut self, symbol: &str, external: bool) {
        self.bytes.push(0xE8);
        let offset = self.bytes.len() as u64;
        let kind = match external {
            true => Kind::Procedure32,
            false => Kind::Relative32,
        };
        self.relocations.push(Relocation {
            offset,
            symbol: symbol.to_owned(),
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

    pub fn transfer(&mut self) {
        self.bytes.extend_from_slice(&[0xFC, 0xF3, 0xA4]);
    }

    pub fn finish(mut self) -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
        for item in &self.patches {
            let Some(dest) = self.marks.get(item.target.0 as usize).copied().flatten() else {
                return Err(Fault::Unimplemented("unbound label".to_owned()));
            };
            let delta = dest as i64 - (item.offset as i64 + 4);
            let bytes = (delta as i32).to_le_bytes();
            self.bytes[item.offset..item.offset + 4].copy_from_slice(&bytes);
        }
        Ok((self.bytes, self.relocations))
    }
}
