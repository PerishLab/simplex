use super::invoke::Call;
use super::{Binding, Context};
use crate::Fault;
use crate::arm::{Precision, Width};
use crate::machine::Location;
use simplex_ir::{Instruction, Operation, Type};

impl Context<'_> {
    pub fn spill(&self, raw: i32) -> i64 {
        let delta = match raw < 0 {
            true => (-raw - 8) as u32,
            false => raw as u32,
        };
        i64::from(self.spills + delta)
    }

    pub fn get(&mut self, vid: u32, fallback: u8) -> u8 {
        match self.plan.locations.get(&vid).copied() {
            Some(Location::General(reg)) => reg,
            Some(Location::Spill(off)) => {
                let offset = self.spill(off);
                self.encoder
                    .load(fallback, (31, offset), (Width::Quad, false));
                fallback
            }
            _ => {
                self.encoder.constant(fallback, 0);
                fallback
            }
        }
    }

    pub fn put(&mut self, vid: u32, source: u8) {
        match self.plan.locations.get(&vid).copied() {
            Some(Location::General(reg)) => self.encoder.copy(reg, source),
            Some(Location::Spill(off)) => {
                let offset = self.spill(off);
                self.encoder.store(source, (31, offset), Width::Quad);
            }
            _ => {}
        }
    }

    pub fn fetch(&mut self, vid: u32, fallback: u8, precision: Precision) -> u8 {
        match self.plan.locations.get(&vid).copied() {
            Some(Location::Float(reg)) => reg,
            Some(Location::Spill(off)) => {
                let offset = self.spill(off);
                self.encoder.sample(fallback, (31, offset), precision);
                fallback
            }
            _ => {
                self.encoder.ingest(fallback, 31, precision);
                fallback
            }
        }
    }

    pub fn place(&mut self, vid: u32, source: u8, precision: Precision) {
        match self.plan.locations.get(&vid).copied() {
            Some(Location::Float(reg)) => self.encoder.duplicate(reg, source, precision),
            Some(Location::Spill(off)) => {
                let offset = self.spill(off);
                self.encoder.deposit(source, (31, offset), precision);
            }
            _ => {}
        }
    }

    pub fn normalize(&mut self, target: u8, source: u8, kind: &Type) {
        match *kind {
            Type::Integer { bits, signed: true } => self.encoder.widen(target, source, bits),
            Type::Integer {
                bits,
                signed: false,
            } => self.encoder.extend(target, source, bits),
            _ => self.encoder.copy(target, source),
        }
    }

    pub fn instruction(&mut self, site: (usize, usize), item: &Instruction) -> Result<(), Fault> {
        let out = item.value;
        match &item.op {
            Operation::Integer { value, kind } => self.integer(*value, kind, out),
            Operation::Float { bits, kind } => self.real(*bits, kind, out),
            Operation::Symbol { name } => self.symbol(name, out),
            Operation::Slot { .. } => self.slot(site, out),
            Operation::Unary { op, value, kind } => self.unary(*op, *value, kind, out),
            Operation::Binary {
                op,
                left,
                right,
                kind,
            } => self.binary(*op, (*left, *right), kind, out),
            Operation::Cast {
                op,
                value,
                from,
                kind,
            } => self.cast(*op, *value, (from, kind), out),
            Operation::Pair { low, high } => self.pair(*low, *high, out),
            Operation::Low { pair, kind } => self.half(*pair, false, kind, out),
            Operation::High { pair, kind } => self.half(*pair, true, kind, out),
            Operation::Offset { base, bytes } => self.offset(*base, *bytes, out),
            Operation::Load { address, kind, .. } => self.load(*address, kind, out),
            Operation::Store {
                address,
                value,
                kind,
                ..
            } => self.store(*address, *value, kind),
            Operation::Copy {
                destination,
                source,
                bytes,
            } => self.duplicate(*destination, *source, *bytes),
            Operation::Acquire { address, kind, .. } => self.acquire(*address, kind, out),
            Operation::Release {
                address,
                value,
                kind,
                ..
            } => self.release(*address, *value, kind),
            Operation::Swap {
                address,
                expected,
                desired,
                kind,
                ..
            } => self.swap((*address, *expected, *desired), kind, (out, &item.kind)),
            Operation::Atomic {
                op,
                address,
                operand,
                kind,
                ..
            } => self.modify(*op, (*address, *operand), kind, out),
            Operation::Fence { .. } => self.encoder.fence(),
            Operation::Call {
                symbol,
                arguments,
                inputs,
                kind,
            } => self.invoke(
                site,
                Ok(symbol),
                Call {
                    arguments,
                    inputs,
                    output: kind,
                    destination: out,
                },
            ),
            Operation::Indirect {
                callee,
                arguments,
                inputs,
                kind,
            } => self.invoke(
                site,
                Err(*callee),
                Call {
                    arguments,
                    inputs,
                    output: kind,
                    destination: out,
                },
            ),
            _ => {
                return Err(Fault::Unsupported(String::from(
                    "unsupported arm operation",
                )));
            }
        }
        Ok(())
    }

    fn integer(&mut self, value: i128, kind: &Type, out: u32) {
        if let Binding::Single(vid) = self.bindings[out as usize] {
            self.encoder.constant(15, (value as i64) as u64);
            self.normalize(15, 15, kind);
            self.put(vid, 15);
        }
    }

    fn real(&mut self, bits: u64, kind: &Type, out: u32) {
        let (Type::Float { bits: width }, Binding::Single(vid)) =
            (kind, self.bindings[out as usize])
        else {
            return;
        };
        let precision = Precision::from(*width);
        if bits == 0 {
            self.encoder.ingest(16, 31, precision);
        } else {
            self.encoder.constant(15, bits);
            self.encoder.ingest(16, 15, precision);
        }
        self.place(vid, 16, precision);
    }

    fn symbol(&mut self, name: &str, out: u32) {
        if let Binding::Single(vid) = self.bindings[out as usize] {
            let (mapped, imported) = self.resolver.symbol(name);
            self.encoder.address(15, &mapped, imported);
            self.put(vid, 15);
        }
    }

    fn slot(&mut self, site: (usize, usize), out: u32) {
        if let Binding::Single(vid) = self.bindings[out as usize] {
            let slot = self.explicit[&site];
            let offset = i64::from(self.locals[slot].offset);
            self.encoder.displace(15, 31, offset);
            self.put(vid, 15);
        }
    }

    fn offset(&mut self, base: u32, bytes: i64, out: u32) {
        if let (Binding::Single(src), Binding::Single(dst)) =
            (self.bindings[base as usize], self.bindings[out as usize])
        {
            let reg = self.get(src, 15);
            self.encoder.displace(15, reg, bytes);
            self.put(dst, 15);
        }
    }

    fn duplicate(&mut self, destination: u32, source: u32, bytes: u32) {
        if let (Binding::Single(d), Binding::Single(s)) = (
            self.bindings[destination as usize],
            self.bindings[source as usize],
        ) {
            let dr = self.get(d, 14);
            self.encoder.copy(14, dr);
            let sr = self.get(s, 17);
            self.encoder.copy(17, sr);
            self.encoder.transfer(14, 17, bytes);
        }
    }
}
