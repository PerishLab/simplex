use crate::Register;
use crate::encode::{Condition, Encoder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    Single,
    Double,
}

impl Precision {
    pub const fn from(bytes: u8) -> Self {
        match bytes {
            4 => Self::Single,
            _ => Self::Double,
        }
    }

    const fn prefix(self) -> u8 {
        match self {
            Self::Single => 0xF3,
            Self::Double => 0xF2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Arithmetic {
    Add = 0x58,
    Mul = 0x59,
    Sub = 0x5C,
    Div = 0x5E,
}

impl Encoder {
    pub fn ingest(&mut self, slot: u8, src: Register, prec: Precision) {
        self.bytes.push(0x66);
        self.rex(prec == Precision::Double, slot, src.code());
        self.bytes
            .extend_from_slice(&[0x0F, 0x6E, Self::pair(3, slot, src.code())]);
    }

    pub fn extract(&mut self, dst: Register, slot: u8, prec: Precision) {
        self.bytes.push(0x66);
        self.rex(prec == Precision::Double, slot, dst.code());
        self.bytes
            .extend_from_slice(&[0x0F, 0x7E, Self::pair(3, slot, dst.code())]);
    }

    pub fn scalar(&mut self, kind: Arithmetic, prec: Precision) {
        self.bytes
            .extend_from_slice(&[prec.prefix(), 0x0F, kind as u8, Self::pair(3, 0, 1)]);
    }

    pub fn order(&mut self, prec: Precision, first: u8, second: u8) {
        if prec == Precision::Double {
            self.bytes.push(0x66);
        }
        self.rex(false, first, second);
        self.bytes
            .extend_from_slice(&[0x0F, 0x2E, Self::pair(3, first, second)]);
    }

    pub fn relation(&mut self, dst: Register, op: &str, prec: Precision) {
        match op {
            "<" | "<=" => self.order(prec, 1, 0),
            _ => self.order(prec, 0, 1),
        }
        match op {
            "==" => {
                self.flag(Register::Rax, Condition::Equal);
                self.flag(Register::Rdx, Condition::Ordered);
                self.bitand(Register::Rax, Register::Rdx);
                self.copy(dst, Register::Rax);
            }
            "!=" => {
                self.flag(Register::Rax, Condition::Distinct);
                self.flag(Register::Rdx, Condition::Parity);
                self.bitor(Register::Rax, Register::Rdx);
                self.copy(dst, Register::Rax);
            }
            "<" | ">" => self.flag(dst, Condition::Above),
            _ => self.flag(dst, Condition::Over),
        }
    }

    pub fn promote(&mut self, dst: Register, src: Register, prec: Precision) {
        self.bytes.push(prec.prefix());
        self.rex(true, 0, src.code());
        self.bytes
            .extend_from_slice(&[0x0F, 0x2A, Self::pair(3, 0, src.code())]);
        self.extract(dst, 0, prec);
    }

    pub fn unsigned(&mut self, dst: Register, src: Register, prec: Precision) {
        let negative = self.fresh();
        let done = self.fresh();
        self.test(src);
        self.bytes.extend_from_slice(&[0x0F, 0x88]);
        let off = self.bytes.len();
        self.bytes.extend_from_slice(&[0; 4]);
        self.promote(dst, src, prec);
        self.jump(done);
        let target = self.bytes.len();
        let delta = (target as i64 - (off as i64 + 4)) as i32;
        self.bytes[off..off + 4].copy_from_slice(&delta.to_le_bytes());
        let _ = negative;
        self.copy(Register::Rax, src);
        self.copy(Register::Rdx, src);
        self.bytes.extend_from_slice(&[0x48, 0xD1, 0xE8]);
        self.bytes.extend_from_slice(&[0x48, 0x83, 0xE2, 0x01]);
        self.bitor(Register::Rax, Register::Rdx);
        self.bytes.push(prec.prefix());
        self.bytes
            .extend_from_slice(&[0x48, 0x0F, 0x2A, Self::pair(3, 0, 0)]);
        self.bytes
            .extend_from_slice(&[prec.prefix(), 0x0F, 0x58, Self::pair(3, 0, 0)]);
        self.extract(dst, 0, prec);
        self.bind(done);
    }

    pub fn truncate(&mut self, dst: Register, src: Register, prec: Precision) {
        self.ingest(0, src, prec);
        self.bytes.push(prec.prefix());
        self.rex(true, dst.code(), 0);
        self.bytes
            .extend_from_slice(&[0x0F, 0x2C, Self::pair(3, dst.code(), 0)]);
    }

    pub fn resize(&mut self, dst: Register, src: Register, target: Precision) {
        let origin = match target {
            Precision::Single => Precision::Double,
            Precision::Double => Precision::Single,
        };
        self.ingest(0, src, origin);
        self.bytes
            .extend_from_slice(&[origin.prefix(), 0x0F, 0x5A, Self::pair(3, 0, 0)]);
        self.extract(dst, 0, target);
    }
}
