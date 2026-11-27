use super::encode::Encoder;
use super::{Address, Condition, Precision, Register};
use simplex_ir::Binary;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Arithmetic {
    Add = 0x58,
    Mul = 0x59,
    Sub = 0x5C,
    Div = 0x5E,
}

impl Encoder {
    pub fn slide(&mut self, dst: u8, src: u8) {
        if dst != src {
            self.bytes.push(0x66);
            self.rex(false, dst, src);
            self.bytes
                .extend_from_slice(&[0x0F, 0x28, Self::pair(3, dst, src)]);
        }
    }

    pub fn fetch(&mut self, dst: u8, addr: Address, prec: Precision) {
        self.bytes.push(prec.prefix());
        self.rex(false, dst, addr.base.code());
        self.bytes.extend_from_slice(&[0x0F, 0x10]);
        self.memory(dst, addr);
    }

    pub fn deposit(&mut self, addr: Address, src: u8, prec: Precision) {
        self.bytes.push(prec.prefix());
        self.rex(false, src, addr.base.code());
        self.bytes.extend_from_slice(&[0x0F, 0x11]);
        self.memory(src, addr);
    }

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

    pub fn scalar(&mut self, kind: Arithmetic, dst: u8, rhs: u8, prec: Precision) {
        self.bytes.push(prec.prefix());
        self.rex(false, dst, rhs);
        self.bytes
            .extend_from_slice(&[0x0F, kind as u8, Self::pair(3, dst, rhs)]);
    }

    pub fn order(&mut self, prec: Precision, first: u8, second: u8) {
        if prec == Precision::Double {
            self.bytes.push(0x66);
        }
        self.rex(false, first, second);
        self.bytes
            .extend_from_slice(&[0x0F, 0x2E, Self::pair(3, first, second)]);
    }

    pub fn relation(&mut self, dst: Register, op: Binary, prec: Precision) {
        match op {
            Binary::Lt | Binary::Le => self.order(prec, 1, 0),
            _ => self.order(prec, 0, 1),
        }
        match op {
            Binary::Eq => {
                self.flag(Register::Rax, Condition::Equal);
                self.flag(Register::Rdx, Condition::Ordered);
                self.mask(Register::Rax, Register::Rdx);
                self.copy(dst, Register::Rax);
            }
            Binary::Ne => {
                self.flag(Register::Rax, Condition::Distinct);
                self.flag(Register::Rdx, Condition::Parity);
                self.unite(Register::Rax, Register::Rdx);
                self.copy(dst, Register::Rax);
            }
            Binary::Lt | Binary::Gt => self.flag(dst, Condition::Above),
            _ => self.flag(dst, Condition::Over),
        }
    }

    pub fn promote(&mut self, dst: u8, src: Register, prec: Precision) {
        self.bytes.push(prec.prefix());
        self.rex(true, dst, src.code());
        self.bytes
            .extend_from_slice(&[0x0F, 0x2A, Self::pair(3, dst, src.code())]);
    }

    pub fn elevate(&mut self, dst: u8, src: Register, prec: Precision) {
        let negative = self.fresh();
        let done = self.fresh();
        self.test(src);
        self.branch(Condition::Sign, negative);
        self.promote(dst, src, prec);
        self.jump(done);
        self.bind(negative);
        self.copy(Register::Rax, src);
        self.copy(Register::Rdx, src);
        self.bytes.extend_from_slice(&[0x48, 0xD1, 0xE8]);
        self.bytes.extend_from_slice(&[0x48, 0x83, 0xE2, 0x01]);
        self.unite(Register::Rax, Register::Rdx);
        self.promote(dst, Register::Rax, prec);
        self.scalar(Arithmetic::Add, dst, dst, prec);
        self.bind(done);
    }

    pub fn truncate(&mut self, dst: Register, src: u8, prec: Precision) {
        self.bytes.push(prec.prefix());
        self.rex(true, dst.code(), src);
        self.bytes
            .extend_from_slice(&[0x0F, 0x2C, Self::pair(3, dst.code(), src)]);
    }

    pub fn demote(&mut self, dst: Register, src: u8, prec: Precision) {
        let large = self.fresh();
        let done = self.fresh();
        let threshold = match prec {
            Precision::Single => 0x5F00_0000_i64,
            Precision::Double => 0x43E0_0000_0000_0000_i64,
        };
        self.constant(Register::Rdx, threshold);
        self.ingest(1, Register::Rdx, prec);
        self.order(prec, src, 1);
        self.branch(Condition::Over, large);
        self.truncate(dst, src, prec);
        self.jump(done);
        self.bind(large);
        self.slide(0, src);
        self.scalar(Arithmetic::Sub, 0, 1, prec);
        self.truncate(dst, 0, prec);
        self.constant(Register::Rdx, i64::MIN);
        self.toggle(dst, Register::Rdx);
        self.bind(done);
    }

    pub fn convert(&mut self, dst: u8, src: u8, target: Precision) {
        let origin = match target {
            Precision::Single => Precision::Double,
            Precision::Double => Precision::Single,
        };
        self.bytes.push(origin.prefix());
        self.rex(false, dst, src);
        self.bytes
            .extend_from_slice(&[0x0F, 0x5A, Self::pair(3, dst, src)]);
    }
}
