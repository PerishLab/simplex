use super::Emitter;
use super::abi::Callee;
use super::shape::Value;
use crate::Fault;
use crate::intel::media::Arithmetic;
use crate::intel::{Address, Condition, Precision, Register, Shift};
use simplex_ir::{Binary, Instruction, Operation, Order, Type, Unary};

impl Emitter<'_> {
    pub(crate) fn instruction(&mut self, inst: &Instruction) -> Result<(), Fault> {
        match &inst.op {
            Operation::Integer { value, kind } => self.constant(inst.value, *value as i64, kind),
            Operation::Float { bits, .. } => self.literal(inst.value, *bits),
            Operation::Symbol { name } => self.symbol(inst.value, name),
            Operation::Slot { .. } => self.slot(inst.value),
            Operation::Unary { op, value, kind } => self.unary(inst.value, *op, *value, kind),
            Operation::Binary {
                op,
                left,
                right,
                kind,
            } => {
                self.binary(inst, *op, (*left, *right), kind);
            }
            Operation::Cast {
                op,
                value,
                from,
                kind,
            } => {
                self.cast(inst.value, *op, *value, (from, kind));
            }
            Operation::Pair { low, high } => self.pair(inst.value, *low, *high),
            Operation::Low { pair, kind } => self.part(inst.value, *pair, kind, false),
            Operation::High { pair, kind } => self.part(inst.value, *pair, kind, true),
            Operation::Offset { base, bytes } => self.offset(inst.value, *base, *bytes),
            Operation::Load { address, kind, .. } | Operation::Acquire { address, kind, .. } => {
                self.load(inst.value, *address, kind);
            }
            Operation::Store {
                address,
                value,
                kind,
                ..
            } => self.store(*address, *value, kind),
            Operation::Release {
                address,
                value,
                kind,
                order,
            } => {
                self.store(*address, *value, kind);
                if *order == Order::Sequential {
                    self.encoder.fence();
                }
            }
            Operation::Copy {
                destination,
                source,
                bytes,
            } => {
                self.copy(*destination, *source, *bytes);
            }
            Operation::Swap {
                address,
                expected,
                desired,
                kind,
                ..
            } => {
                self.swap(inst, (*address, *expected, *desired), kind);
            }
            Operation::Atomic {
                op,
                address,
                operand,
                kind,
                ..
            } => {
                self.atomic(inst.value, *op, (*address, *operand), kind);
            }
            Operation::Fence { .. } => self.encoder.fence(),
            Operation::Call {
                symbol,
                arguments,
                inputs,
                ..
            } => {
                let (mapped, _) = self.symbols.resolve(symbol);
                self.invoke(inst, Callee::Direct(mapped), (arguments, inputs));
            }
            Operation::Indirect {
                callee,
                arguments,
                inputs,
                ..
            } => {
                self.invoke(inst, Callee::Indirect(*callee), (arguments, inputs));
            }
            Operation::Spawn { .. }
            | Operation::Yield
            | Operation::Await { .. }
            | Operation::All { .. }
            | Operation::Race { .. } => {
                return Err(Fault::Unsupported("unprepared fiber operation".to_owned()));
            }
        }
        Ok(())
    }

    fn constant(&mut self, out: u32, val: i64, kind: &Type) {
        if let Value::General(dst) = self.frame.value(out) {
            self.encoder.constant(Register::Rax, val);
            self.normalize(Register::Rax, kind);
            self.write(dst, Register::Rax);
        }
    }

    fn literal(&mut self, out: u32, bits: u64) {
        if let Value::Float { reg, prec } = self.frame.value(out) {
            self.encoder.constant(Register::Rax, bits as i64);
            self.encoder.ingest(0, Register::Rax, prec);
            self.deposit(reg, 0);
        }
    }

    fn symbol(&mut self, out: u32, name: &str) {
        let Value::General(dst) = self.frame.value(out) else {
            return;
        };
        let (mapped, imported) = self.symbols.resolve(name);
        match imported {
            true => self.encoder.global(Register::Rax, &mapped),
            false => self.encoder.symbol(Register::Rax, &mapped),
        }
        self.write(dst, Register::Rax);
    }

    fn slot(&mut self, out: u32) {
        if let Value::General(dst) = self.frame.value(out) {
            let idx = self.frame.allocation(out);
            let addr = Address::new(Register::Rbp, self.frame.base(idx));
            self.encoder.address(Register::Rax, addr);
            self.write(dst, Register::Rax);
        }
    }

    fn unary(&mut self, out: u32, op: Unary, src: u32, kind: &Type) {
        match (op, self.frame.value(out), self.frame.value(src)) {
            (Unary::Neg, Value::Float { reg: d, prec }, Value::Float { reg: s, .. }) => {
                let sign = match prec {
                    Precision::Single => 1i64 << 31,
                    Precision::Double => i64::MIN,
                };
                self.fetch(0, s);
                self.encoder.extract(Register::Rax, 0, prec);
                self.encoder.constant(Register::Rdx, sign);
                self.encoder.toggle(Register::Rax, Register::Rdx);
                self.encoder.ingest(0, Register::Rax, prec);
                self.deposit(d, 0);
            }
            (Unary::Neg, Value::General(d), Value::General(s)) => {
                self.read(Register::Rax, s);
                self.normalize(Register::Rax, kind);
                self.encoder.negate(Register::Rax);
                self.normalize(Register::Rax, kind);
                self.write(d, Register::Rax);
            }
            (Unary::Not, Value::General(d), Value::General(s)) => {
                self.invert(d, s, kind);
            }
            _ => {}
        }
    }

    fn invert(&mut self, dst: u32, src: u32, kind: &Type) {
        self.read(Register::Rax, src);
        match kind {
            Type::Integer { bits: 1, .. } => {
                self.normalize(Register::Rax, kind);
                self.encoder.constant(Register::Rdx, 1);
                self.encoder.toggle(Register::Rax, Register::Rdx);
            }
            _ => {
                self.encoder.invert(Register::Rax);
                self.normalize(Register::Rax, kind);
            }
        }
        self.write(dst, Register::Rax);
    }

    fn binary(&mut self, inst: &Instruction, op: Binary, args: (u32, u32), domain: &Type) {
        if let Type::Float { bits } = domain {
            self.float(inst, op, args, Precision::bits(*bits));
            return;
        }
        let (Value::General(l), Value::General(r), Value::General(d)) = (
            self.frame.value(args.0),
            self.frame.value(args.1),
            self.frame.value(inst.value),
        ) else {
            return;
        };
        self.read(Register::Rax, l);
        self.normalize(Register::Rax, domain);
        self.read(Register::Rcx, r);
        self.normalize(Register::Rcx, domain);
        let signed = matches!(domain, Type::Integer { signed: true, .. });
        match op {
            Binary::Add => self.encoder.add(Register::Rax, Register::Rcx),
            Binary::Sub => self.encoder.sub(Register::Rax, Register::Rcx),
            Binary::Mul => self.encoder.multiply(Register::Rax, Register::Rcx),
            Binary::Div => self.encoder.divide(signed),
            Binary::Rem => {
                self.encoder.divide(signed);
                self.encoder.copy(Register::Rax, Register::Rdx);
            }
            Binary::And => self.encoder.mask(Register::Rax, Register::Rcx),
            Binary::Or => self.encoder.unite(Register::Rax, Register::Rcx),
            Binary::Xor => self.encoder.toggle(Register::Rax, Register::Rcx),
            Binary::Shl => self.encoder.shift(Shift::Left, Register::Rax),
            Binary::Shr => {
                let dir = match signed {
                    true => Shift::Arithmetic,
                    false => Shift::Logical,
                };
                self.encoder.shift(dir, Register::Rax);
            }
            _ => {
                self.encoder.compare(Register::Rax, Register::Rcx);
                self.encoder.flag(Register::Rax, relation(op, signed));
            }
        }
        self.normalize(Register::Rax, &inst.kind);
        self.write(d, Register::Rax);
    }

    fn float(&mut self, inst: &Instruction, op: Binary, args: (u32, u32), prec: Precision) {
        let (Value::Float { reg: l, .. }, Value::Float { reg: r, .. }) =
            (self.frame.value(args.0), self.frame.value(args.1))
        else {
            return;
        };
        self.fetch(0, l);
        self.fetch(1, r);
        let arith = match op {
            Binary::Add => Some(Arithmetic::Add),
            Binary::Sub => Some(Arithmetic::Sub),
            Binary::Mul => Some(Arithmetic::Mul),
            Binary::Div => Some(Arithmetic::Div),
            _ => None,
        };
        match (arith, op, self.frame.value(inst.value)) {
            (Some(kind), _, Value::Float { reg: d, .. }) => {
                self.encoder.scalar(kind, 0, 1, prec);
                self.deposit(d, 0);
            }
            (None, Binary::Rem, Value::Float { reg: d, .. }) => {
                self.encoder.scalar(Arithmetic::Div, 0, 1, prec);
                self.encoder.truncate(Register::Rax, 0, prec);
                self.encoder.promote(0, Register::Rax, prec);
                self.encoder.scalar(Arithmetic::Mul, 0, 1, prec);
                self.fetch(1, l);
                self.encoder.scalar(Arithmetic::Sub, 1, 0, prec);
                self.deposit(d, 1);
            }
            (None, _, Value::General(d)) => {
                self.encoder.relation(Register::Rax, op, prec);
                self.write(d, Register::Rax);
            }
            _ => {}
        }
    }
}

const fn relation(op: Binary, signed: bool) -> Condition {
    match (op, signed) {
        (Binary::Eq, _) => Condition::Equal,
        (Binary::Ne, _) => Condition::Distinct,
        (Binary::Lt, true) => Condition::Less,
        (Binary::Lt, false) => Condition::Below,
        (Binary::Le, true) => Condition::Most,
        (Binary::Le, false) => Condition::Under,
        (Binary::Gt, true) => Condition::Greater,
        (Binary::Gt, false) => Condition::Above,
        (Binary::Ge, true) => Condition::Least,
        _ => Condition::Over,
    }
}
