use super::encode::{Encoder, Patch};
use super::{Address, Condition, Label, Register, Shift, Width};

impl Encoder {
    pub fn trap(&mut self) {
        self.bytes.extend_from_slice(&[0x0F, 0x0B]);
    }
    pub fn kernel(&mut self) {
        self.bytes.extend_from_slice(&[0x0F, 0x05]);
    }
    pub fn align(&mut self) {
        self.bytes.extend_from_slice(&[0x48, 0x83, 0xE4, 0xF0]);
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
    pub fn mask(&mut self, dst: Register, rhs: Register) {
        self.binary(0x21, dst, rhs);
    }
    pub fn unite(&mut self, dst: Register, rhs: Register) {
        self.binary(0x09, dst, rhs);
    }
    pub fn toggle(&mut self, dst: Register, rhs: Register) {
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

    pub fn displace(&mut self, left: bool, dst: Register, bits: u8) {
        if bits == 0 {
            return;
        }
        let mode = match left {
            true => 4,
            false => 5,
        };
        self.rex(true, 0, dst.code());
        self.bytes
            .extend_from_slice(&[0xC1, Self::pair(3, mode, dst.code()), bits]);
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

    pub fn fence(&mut self) {
        self.bytes.extend_from_slice(&[0x0F, 0xAE, 0xF0]);
    }

    pub fn exchange(&mut self, addr: Address, reg: Register, width: Width) {
        let r = reg.code();
        let b = addr.base.code();
        match width {
            Width::Byte => {
                self.tag(false, r, b, r >= 4);
                self.bytes.push(0x86);
            }
            Width::Word => {
                self.bytes.push(0x66);
                self.rex(false, r, b);
                self.bytes.push(0x87);
            }
            Width::Double => {
                self.rex(false, r, b);
                self.bytes.push(0x87);
            }
            Width::Quad => {
                self.rex(true, r, b);
                self.bytes.push(0x87);
            }
        }
        self.memory(r, addr);
    }

    pub fn atomic(&mut self, addr: Address, reg: Register, width: Width, opcode: u8) {
        let r = reg.code();
        let b = addr.base.code();
        self.bytes.push(0xF0);
        match width {
            Width::Byte => {
                self.tag(false, r, b, r >= 4);
                self.bytes.extend_from_slice(&[0x0F, opcode]);
            }
            Width::Word => {
                self.bytes.push(0x66);
                self.rex(false, r, b);
                self.bytes.extend_from_slice(&[0x0F, opcode + 1]);
            }
            Width::Double => {
                self.rex(false, r, b);
                self.bytes.extend_from_slice(&[0x0F, opcode + 1]);
            }
            Width::Quad => {
                self.rex(true, r, b);
                self.bytes.extend_from_slice(&[0x0F, opcode + 1]);
            }
        }
        self.memory(r, addr);
    }

    pub fn swap(&mut self, addr: Address, desired: Register, width: Width) {
        self.atomic(addr, desired, width, 0xB0);
    }

    pub fn accumulate(&mut self, addr: Address, delta: Register, width: Width) {
        self.atomic(addr, delta, width, 0xC0);
    }

    pub fn pack(&mut self, dst: Register, addr: Address, bytes: u32) {
        match bytes {
            0 => self.constant(dst, 0),
            1 | 2 | 4 | 8 => self.load(dst, addr, Width::bytes(bytes)),
            n => self.gather(dst, addr, n.min(8)),
        }
    }

    fn gather(&mut self, dst: Register, addr: Address, count: u32) {
        self.constant(dst, 0);
        for idx in (0..count).rev() {
            self.displace(true, dst, 8);
            let pos = Address::new(addr.base, addr.offset + idx as i32);
            self.load(Register::R11, pos, Width::Byte);
            self.unite(dst, Register::R11);
        }
    }

    pub fn unpack(&mut self, addr: Address, src: Register, bytes: u32) {
        match bytes {
            0 => {}
            1 | 2 | 4 | 8 => self.store(addr, src, Width::bytes(bytes)),
            n => self.scatter(addr, src, n.min(8)),
        }
    }

    fn scatter(&mut self, addr: Address, src: Register, count: u32) {
        self.copy(Register::Rax, src);
        let mut cursor = addr.offset;
        let mut rest = count;
        for step in [4u32, 2, 1] {
            if rest >= step {
                self.store(
                    Address::new(addr.base, cursor),
                    Register::Rax,
                    Width::bytes(step),
                );
                self.displace(false, Register::Rax, (step * 8) as u8);
                cursor += step as i32;
                rest -= step;
            }
        }
    }

    pub fn replicate(&mut self, dst: Register, src: Register, bytes: u32) {
        if bytes == 0 || dst == src {
            return;
        }
        let mut cursor = 0i32;
        let mut rest = bytes;
        if rest > 64 {
            let head = self.fresh();
            self.constant(Register::Rcx, i64::from(rest / 8));
            self.bind(head);
            self.load(Register::Rax, Address::new(src, 0), Width::Quad);
            self.store(Address::new(dst, 0), Register::Rax, Width::Quad);
            self.adjust(src, 8);
            self.adjust(dst, 8);
            self.adjust(Register::Rcx, -1);
            self.branch(Condition::Distinct, head);
            rest %= 8;
        }
        while rest >= 8 {
            self.load(Register::Rax, Address::new(src, cursor), Width::Quad);
            self.store(Address::new(dst, cursor), Register::Rax, Width::Quad);
            cursor += 8;
            rest -= 8;
        }
        for step in [4u32, 2, 1] {
            if rest >= step {
                let width = Width::bytes(step);
                self.load(Register::Rax, Address::new(src, cursor), width);
                self.store(Address::new(dst, cursor), Register::Rax, width);
                cursor += step as i32;
                rest -= step;
            }
        }
    }
}
