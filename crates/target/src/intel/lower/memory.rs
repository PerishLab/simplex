use super::Emitter;
use super::shape::Value;
use crate::intel::{Address, Condition, Register, Width};
use simplex_ir::{Atomic, Instruction, Type};

impl Emitter<'_> {
    pub(crate) fn bitcast(&mut self, out: u32, src: u32, target: &Type) {
        match (self.frame.value(out), self.frame.value(src)) {
            (Value::General(d), Value::General(s)) => {
                self.read(Register::Rax, s);
                self.normalize(Register::Rax, target);
                self.write(d, Register::Rax);
            }
            (Value::Float { reg: d, prec }, Value::General(s)) => {
                self.read(Register::Rax, s);
                self.encoder.ingest(0, Register::Rax, prec);
                self.deposit(d, 0);
            }
            (Value::General(d), Value::Float { reg: s, prec }) => {
                self.fetch(0, s);
                self.encoder.extract(Register::Rax, 0, prec);
                self.normalize(Register::Rax, target);
                self.write(d, Register::Rax);
            }
            (Value::Float { reg: d, .. }, Value::Float { reg: s, .. }) => {
                self.fetch(0, s);
                self.deposit(d, 0);
            }
            _ => {}
        }
    }

    pub(crate) fn pair(&mut self, out: u32, low: u32, high: u32) {
        let Value::Pair { low: dl, high: dh } = self.frame.value(out) else {
            return;
        };
        self.word(Register::Rax, low);
        self.word(Register::R10, high);
        self.write(dl, Register::Rax);
        self.write(dh, Register::R10);
    }

    fn word(&mut self, dst: Register, id: u32) {
        match self.frame.value(id) {
            Value::General(r) => self.read(dst, r),
            Value::Float { reg, prec } => {
                self.fetch(0, reg);
                self.encoder.extract(dst, 0, prec);
            }
            _ => self.encoder.constant(dst, 0),
        }
    }

    pub(crate) fn part(&mut self, out: u32, pair: u32, kind: &Type, upper: bool) {
        let Value::Pair { low, high } = self.frame.value(pair) else {
            return;
        };
        let chosen = match upper {
            true => high,
            false => low,
        };
        self.read(Register::Rax, chosen);
        match self.frame.value(out) {
            Value::General(d) => {
                self.normalize(Register::Rax, kind);
                self.write(d, Register::Rax);
            }
            Value::Float { reg, prec } => {
                self.encoder.ingest(0, Register::Rax, prec);
                self.deposit(reg, 0);
            }
            _ => {}
        }
    }

    pub(crate) fn offset(&mut self, out: u32, base: u32, bytes: i64) {
        let (Value::General(d), Value::General(b)) =
            (self.frame.value(out), self.frame.value(base))
        else {
            return;
        };
        self.read(Register::Rax, b);
        match i32::try_from(bytes) {
            Ok(disp) => self
                .encoder
                .address(Register::Rax, Address::new(Register::Rax, disp)),
            Err(_) => {
                self.encoder.constant(Register::Rcx, bytes);
                self.encoder.add(Register::Rax, Register::Rcx);
            }
        }
        self.write(d, Register::Rax);
    }

    pub(crate) fn copy(&mut self, destination: u32, source: u32, bytes: u32) {
        if let (Value::General(d), Value::General(s)) =
            (self.frame.value(destination), self.frame.value(source))
        {
            self.read(Register::R10, d);
            self.read(Register::R11, s);
            self.encoder.replicate(Register::R10, Register::R11, bytes);
        }
    }

    pub(crate) fn load(&mut self, out: u32, address: u32, kind: &Type) {
        let Value::General(ptr) = self.frame.value(address) else {
            return;
        };
        match (self.frame.value(out), kind) {
            (Value::General(d), Type::Integer { bits, signed: true }) => {
                self.read(Register::R10, ptr);
                self.encoder.signed(
                    Register::Rax,
                    Address::new(Register::R10, 0),
                    Width::bits(*bits),
                );
                self.write(d, Register::Rax);
            }
            (
                Value::General(d),
                Type::Integer {
                    bits,
                    signed: false,
                },
            ) => {
                self.read(Register::R10, ptr);
                self.encoder.load(
                    Register::Rax,
                    Address::new(Register::R10, 0),
                    Width::bits(*bits),
                );
                self.normalize(Register::Rax, kind);
                self.write(d, Register::Rax);
            }
            (Value::General(d), Type::Pointer) => {
                self.read(Register::R10, ptr);
                self.encoder
                    .load(Register::Rax, Address::new(Register::R10, 0), Width::Quad);
                self.write(d, Register::Rax);
            }
            (Value::Float { reg, prec }, _) => {
                self.read(Register::R10, ptr);
                self.encoder.fetch(0, Address::new(Register::R10, 0), prec);
                self.deposit(reg, 0);
            }
            (Value::Pair { low, high }, _) => {
                self.read(Register::R10, ptr);
                self.encoder
                    .load(Register::Rax, Address::new(Register::R10, 0), Width::Quad);
                self.encoder
                    .load(Register::Rcx, Address::new(Register::R10, 8), Width::Quad);
                self.write(low, Register::Rax);
                self.write(high, Register::Rcx);
            }
            (Value::Aggregate { slot, size }, _) => {
                self.read(Register::R11, ptr);
                let dst = Address::new(Register::Rbp, self.frame.base(slot));
                self.encoder.address(Register::R10, dst);
                self.encoder.replicate(Register::R10, Register::R11, size);
            }
            _ => {}
        }
    }

    pub(crate) fn store(&mut self, address: u32, value: u32, kind: &Type) {
        let Value::General(ptr) = self.frame.value(address) else {
            return;
        };
        match (self.frame.value(value), kind) {
            (Value::General(v), Type::Integer { bits, .. }) => {
                self.read(Register::R10, ptr);
                self.read(Register::Rax, v);
                self.encoder.store(
                    Address::new(Register::R10, 0),
                    Register::Rax,
                    Width::bits(*bits),
                );
            }
            (Value::General(v), Type::Pointer) => {
                self.read(Register::R10, ptr);
                self.read(Register::Rax, v);
                self.encoder
                    .store(Address::new(Register::R10, 0), Register::Rax, Width::Quad);
            }
            (Value::Float { reg, prec }, _) => {
                self.read(Register::R10, ptr);
                self.fetch(0, reg);
                self.encoder
                    .deposit(Address::new(Register::R10, 0), 0, prec);
            }
            (Value::Pair { low, high }, _) => {
                self.read(Register::R10, ptr);
                self.read(Register::Rax, low);
                self.encoder
                    .store(Address::new(Register::R10, 0), Register::Rax, Width::Quad);
                self.read(Register::Rax, high);
                self.encoder
                    .store(Address::new(Register::R10, 8), Register::Rax, Width::Quad);
            }
            (Value::Aggregate { slot, size }, _) => {
                self.read(Register::R10, ptr);
                let src = Address::new(Register::Rbp, self.frame.base(slot));
                self.encoder.address(Register::R11, src);
                self.encoder.replicate(Register::R10, Register::R11, size);
            }
            _ => {}
        }
    }

    pub(crate) fn swap(&mut self, inst: &Instruction, args: (u32, u32, u32), kind: &Type) {
        let (Value::General(ptr), Value::General(exp), Value::General(des)) = (
            self.frame.value(args.0),
            self.frame.value(args.1),
            self.frame.value(args.2),
        ) else {
            return;
        };
        self.read(Register::R10, ptr);
        self.read(Register::Rax, exp);
        self.read(Register::Rcx, des);
        self.encoder
            .swap(Address::new(Register::R10, 0), Register::Rcx, width(kind));
        match (self.frame.value(inst.value), &inst.kind) {
            (Value::Pair { low, high }, _) => {
                self.encoder.flag(Register::Rcx, Condition::Equal);
                self.normalize(Register::Rax, kind);
                self.write(low, Register::Rax);
                self.write(high, Register::Rcx);
            }
            (Value::General(d), Type::Integer { bits: 1, .. }) => {
                self.encoder.flag(Register::Rax, Condition::Equal);
                self.write(d, Register::Rax);
            }
            (Value::General(d), _) => {
                self.normalize(Register::Rax, kind);
                self.write(d, Register::Rax);
            }
            _ => {}
        }
    }

    pub(crate) fn atomic(&mut self, out: u32, op: Atomic, args: (u32, u32), kind: &Type) {
        let (Value::General(d), Value::General(ptr), Value::General(val)) = (
            self.frame.value(out),
            self.frame.value(args.0),
            self.frame.value(args.1),
        ) else {
            return;
        };
        self.read(Register::R10, ptr);
        self.read(Register::Rax, val);
        let addr = Address::new(Register::R10, 0);
        let w = width(kind);
        match op {
            Atomic::Exchange => self.encoder.exchange(addr, Register::Rax, w),
            Atomic::Add => self.encoder.accumulate(addr, Register::Rax, w),
            Atomic::Sub => {
                self.encoder.negate(Register::Rax);
                self.encoder.accumulate(addr, Register::Rax, w);
            }
        }
        self.normalize(Register::Rax, kind);
        self.write(d, Register::Rax);
    }
}

const fn width(kind: &Type) -> Width {
    match kind {
        Type::Integer { bits, .. } => Width::bits(*bits),
        _ => Width::Quad,
    }
}
