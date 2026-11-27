use super::{Binding, Context};
use crate::arm::{Condition, Precision, Width};
use simplex_ir::{Atomic, Type};

const fn width(kind: &Type) -> (Width, u8, bool) {
    match *kind {
        Type::Integer { bits: 1, .. } => (Width::Byte, 1, false),
        Type::Integer { bits: 8, signed } => (Width::Byte, 8, signed),
        Type::Integer { bits: 16, signed } => (Width::Half, 16, signed),
        Type::Integer { bits: 32, signed } => (Width::Word, 32, signed),
        Type::Float { bits: 32 } => (Width::Word, 32, false),
        _ => (Width::Quad, 64, false),
    }
}

impl Context<'_> {
    pub fn load(&mut self, address: u32, kind: &Type, out: u32) {
        let Binding::Single(addr) = self.bindings[address as usize] else {
            return;
        };
        let base = self.get(addr, 14);
        self.encoder.copy(14, base);
        match (kind, self.bindings[out as usize]) {
            (Type::Void, _) | (_, Binding::Empty) => {}
            (Type::Float { bits }, Binding::Single(dst)) => {
                let precision = Precision::from(*bits);
                self.encoder.sample(16, (14, 0), precision);
                self.place(dst, 16, precision);
            }
            (Type::Pair, Binding::Pair(low, high)) => {
                self.encoder.load(15, (14, 0), (Width::Quad, false));
                self.put(low, 15);
                self.encoder.load(15, (14, 8), (Width::Quad, false));
                self.put(high, 15);
            }
            (Type::Aggregate { size, .. }, Binding::Slot(slot)) => {
                let offset = i64::from(self.locals[slot].offset);
                self.encoder.displace(17, 31, offset);
                self.encoder.transfer(17, 14, *size);
            }
            (_, Binding::Single(dst)) => {
                let (access, bits, signed) = width(kind);
                self.encoder.load(15, (14, 0), (access, signed));
                if bits == 1 {
                    self.encoder.extend(15, 15, 1);
                }
                self.put(dst, 15);
            }
            _ => {}
        }
    }

    pub fn store(&mut self, address: u32, value: u32, kind: &Type) {
        let Binding::Single(addr) = self.bindings[address as usize] else {
            return;
        };
        let base = self.get(addr, 14);
        self.encoder.copy(14, base);
        match (kind, self.bindings[value as usize]) {
            (Type::Void, _) | (_, Binding::Empty) => {}
            (Type::Float { bits }, Binding::Single(src)) => {
                let precision = Precision::from(*bits);
                let freg = self.fetch(src, 16, precision);
                self.encoder.deposit(freg, (14, 0), precision);
            }
            (Type::Pair, Binding::Pair(low, high)) => {
                let lv = self.get(low, 15);
                self.encoder.store(lv, (14, 0), Width::Quad);
                let hv = self.get(high, 15);
                self.encoder.store(hv, (14, 8), Width::Quad);
            }
            (Type::Aggregate { size, .. }, Binding::Slot(slot)) => {
                let offset = i64::from(self.locals[slot].offset);
                self.encoder.displace(17, 31, offset);
                self.encoder.transfer(14, 17, *size);
            }
            (_, Binding::Single(src)) => {
                let (access, _, _) = width(kind);
                let reg = self.get(src, 15);
                self.encoder.store(reg, (14, 0), access);
            }
            _ => {}
        }
    }

    pub fn acquire(&mut self, address: u32, kind: &Type, out: u32) {
        let Binding::Single(addr) = self.bindings[address as usize] else {
            return;
        };
        let base = self.get(addr, 14);
        let (access, _, _) = width(kind);
        self.encoder.acquire(15, base, access);
        match (kind, self.bindings[out as usize]) {
            (Type::Float { bits }, Binding::Single(dst)) => {
                let precision = Precision::from(*bits);
                self.encoder.ingest(16, 15, precision);
                self.place(dst, 16, precision);
            }
            (_, Binding::Single(dst)) => {
                self.normalize(15, 15, kind);
                self.put(dst, 15);
            }
            _ => {}
        }
    }

    pub fn release(&mut self, address: u32, value: u32, kind: &Type) {
        let Binding::Single(addr) = self.bindings[address as usize] else {
            return;
        };
        let base = self.get(addr, 14);
        self.encoder.copy(14, base);
        let (access, _, _) = width(kind);
        match (kind, self.bindings[value as usize]) {
            (Type::Float { bits }, Binding::Single(src)) => {
                let precision = Precision::from(*bits);
                let freg = self.fetch(src, 16, precision);
                self.encoder.extract(15, freg, precision);
                self.encoder.release(15, 14, access);
            }
            (_, Binding::Single(src)) => {
                let reg = self.get(src, 15);
                self.encoder.release(reg, 14, access);
            }
            _ => {}
        }
    }

    pub fn swap(&mut self, operands: (u32, u32, u32), kind: &Type, result: (u32, &Type)) {
        let (address, expected, desired) = operands;
        let (out, shape) = result;
        let Binding::Single(addr) = self.bindings[address as usize] else {
            return;
        };
        let base = self.get(addr, 14);
        self.encoder.copy(14, base);
        let (access, bits, _) = width(kind);
        let retry = self.encoder.fresh();
        let failed = self.encoder.fresh();
        let done = self.encoder.fresh();
        self.encoder.bind(retry);
        self.encoder.exclusive(16, 14, access);
        self.raw(expected, 15);
        self.encoder.extend(15, 15, bits);
        self.encoder.compare(16, 15);
        self.encoder.branch(Condition::Distinct, failed);
        self.raw(desired, 15);
        self.encoder.commit(17, (15, 14), access);
        self.encoder.zero(17, true, retry);
        self.encoder.constant(15, 1);
        self.encoder.jump(done);
        self.encoder.bind(failed);
        self.encoder.emit(0xD503_3F5F);
        self.encoder.constant(15, 0);
        self.encoder.bind(done);
        self.settle(kind, shape, out);
    }

    fn settle(&mut self, kind: &Type, shape: &Type, out: u32) {
        match (shape, self.bindings[out as usize]) {
            (Type::Integer { bits: 1, .. }, Binding::Single(dst)) => self.put(dst, 15),
            (Type::Pair, Binding::Pair(low, high)) => {
                self.normalize(16, 16, kind);
                self.put(low, 16);
                self.put(high, 15);
            }
            (Type::Float { bits }, Binding::Single(dst)) => {
                let precision = Precision::from(*bits);
                self.encoder.ingest(16, 16, precision);
                self.place(dst, 16, precision);
            }
            (_, Binding::Single(dst)) => {
                self.normalize(16, 16, kind);
                self.put(dst, 16);
            }
            _ => {}
        }
    }

    fn raw(&mut self, id: u32, target: u8) {
        match (&self.kinds[id as usize].clone(), self.bindings[id as usize]) {
            (Type::Float { bits }, Binding::Single(vid)) => {
                let precision = Precision::from(*bits);
                let freg = self.fetch(vid, 17, precision);
                self.encoder.extract(target, freg, precision);
            }
            (_, Binding::Single(vid)) => {
                let reg = self.get(vid, target);
                self.encoder.copy(target, reg);
            }
            _ => self.encoder.constant(target, 0),
        }
    }

    pub fn modify(&mut self, op: Atomic, operands: (u32, u32), kind: &Type, out: u32) {
        let (address, operand) = operands;
        let Binding::Single(addr) = self.bindings[address as usize] else {
            return;
        };
        let base = self.get(addr, 14);
        self.encoder.copy(14, base);
        let (access, _, _) = width(kind);
        let retry = self.encoder.fresh();
        self.encoder.bind(retry);
        self.encoder.exclusive(16, 14, access);
        self.raw(operand, 15);
        match op {
            Atomic::Exchange => {}
            Atomic::Add => self.encoder.add(15, 16, 15),
            Atomic::Sub => self.encoder.sub(15, 16, 15),
        }
        self.encoder.commit(17, (15, 14), access);
        self.encoder.zero(17, true, retry);
        if let Binding::Single(dst) = self.bindings[out as usize] {
            self.normalize(16, 16, kind);
            self.put(dst, 16);
        }
    }
}
