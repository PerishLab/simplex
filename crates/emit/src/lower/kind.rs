use super::Function;
use super::abi::{Signature, Value};
use crate::Fault;
use crate::encode::{Condition, Label, Width};
use crate::machine::linear::{Access, Binary, Step};
use simplex_hir::{Expression, Type};

pub fn infer(expr: &Expression) -> Type {
    match expr {
        Expression::Void | Expression::Yield => Type::Void,
        Expression::Bool(_) => Type::Bool,
        Expression::Bytes(_) => Type::Slice(Box::new(Type::Unsigned(1))),
        Expression::Length(_) => Type::Unsigned(8),
        Expression::Int(_, k)
        | Expression::Float(_, k)
        | Expression::Symbol(_, k)
        | Expression::Local(_, k)
        | Expression::Unary { kind: k, .. }
        | Expression::Binary { kind: k, .. }
        | Expression::Cast { kind: k, .. }
        | Expression::Borrow { kind: k, .. }
        | Expression::Load { kind: k, .. }
        | Expression::Field { kind: k, .. }
        | Expression::Index { kind: k, .. }
        | Expression::Window { kind: k, .. }
        | Expression::Record { kind: k, .. }
        | Expression::Variant { kind: k, .. }
        | Expression::List { kind: k, .. }
        | Expression::Buffer { kind: k, .. }
        | Expression::Coerce { kind: k, .. }
        | Expression::Call { kind: k, .. }
        | Expression::Virtual { kind: k, .. }
        | Expression::Indirect { kind: k, .. }
        | Expression::Block { kind: k, .. }
        | Expression::Branch { kind: k, .. }
        | Expression::Switch { kind: k, .. }
        | Expression::Loop { kind: k, .. }
        | Expression::Await { kind: k, .. }
        | Expression::All { kind: k, .. }
        | Expression::Race { kind: k, .. } => k.clone(),
    }
}

impl Function<'_, '_> {
    pub fn classify(&self, expr: &Expression) -> Type {
        match expr {
            Expression::Call { symbol, kind, .. } if *kind == Type::Void => {
                self.resolve(symbol).output
            }
            other => infer(other),
        }
    }

    pub fn place(&mut self, expr: &Expression) -> Result<u32, Fault> {
        match expr {
            Expression::Local(name, _) => self.address(name),
            Expression::Load { pointer, .. } => self.scalar(pointer),
            Expression::Field { base, offset, .. } => {
                let root = self.origin(base)?;
                Ok(Signature::displace(&mut self.proc, root, *offset as i32))
            }
            Expression::Index { base, index, kind } => {
                let (data, _) = self.sequence(base)?;
                let idx = self.scalar(index)?;
                let stride = self.sig().footprint(kind).size.max(1) as i64;
                let scale = Signature::constant(&mut self.proc, stride);
                let byte = self.proc.value();
                self.proc.emit(Step::Binary {
                    kind: Binary::Mul,
                    dst: byte,
                    lhs: idx,
                    rhs: scale,
                });
                Ok(self.advance(data, byte))
            }
            other => {
                let kind = self.classify(other);
                let val = self.expression(other)?;
                Ok(self.sig().materialize(&mut self.proc, val, &kind))
            }
        }
    }

    pub fn origin(&mut self, expr: &Expression) -> Result<u32, Fault> {
        match self.classify(expr) {
            Type::Pointer(_) => self.scalar(expr),
            _ => self.place(expr),
        }
    }

    pub fn sequence(&mut self, expr: &Expression) -> Result<(u32, u32), Fault> {
        match self.classify(expr) {
            Type::Pointer(inner) => {
                let ptr = self.scalar(expr)?;
                match self.sig().load(&mut self.proc, ptr, &inner) {
                    Value::Pair(data, len) => Ok((data, len)),
                    _ => Err(Fault::Unimplemented("invalid sequence pointer".to_owned())),
                }
            }
            _ => match self.expression(expr)? {
                Value::Pair(data, len) => Ok((data, len)),
                _ => Err(Fault::Unimplemented("expected slice or list".to_owned())),
            },
        }
    }

    pub(crate) fn advance(&mut self, base: u32, delta: u32) -> u32 {
        let dst = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Add,
            dst,
            lhs: base,
            rhs: delta,
        });
        dst
    }

    pub(crate) fn slices(
        &mut self,
        operator: &str,
        pair: (&Expression, &Expression),
        elem: &Type,
    ) -> Result<Value, Fault> {
        let (lp, ll) = self.sequence(pair.0)?;
        let (rp, rl) = self.sequence(pair.1)?;
        let slot = self.proc.reserve(8, 8);
        let out = Signature::base(&mut self.proc, slot, 0);
        let unequal = operator == "!=";
        let fail = Signature::constant(&mut self.proc, i64::from(unequal));
        let pass = Signature::constant(&mut self.proc, i64::from(!unequal));
        self.proc.emit(Step::Store {
            base: out,
            offset: 0,
            src: fail,
            width: Width::Quad,
        });
        let done = self.proc.label();
        let same = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Compare(Condition::Equal),
            dst: same,
            lhs: ll,
            rhs: rl,
        });
        self.proc.emit(Step::Branch {
            cond: same,
            truth: false,
            target: done,
        });
        let stride = self.sig().footprint(elem).size.max(1) as i64;
        let factor = Signature::constant(&mut self.proc, stride);
        let total = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Mul,
            dst: total,
            lhs: ll,
            rhs: factor,
        });
        self.scan((lp, rp, total), (out, pass, done));
        self.proc.emit(Step::Mark(done));
        Ok(Value::Scalar(self.sig().fetch(&mut self.proc, out, 0, 1)))
    }

    fn scan(&mut self, span: (u32, u32, u32), exit: (u32, u32, Label)) {
        let idx = self.proc.reserve(8, 8);
        let cursor = Signature::base(&mut self.proc, idx, 0);
        self.proc.emit(Step::Zero {
            dst: cursor,
            bytes: 8,
        });
        let head = self.proc.label();
        let body = self.proc.label();
        self.proc.emit(Step::Mark(head));
        let pos = self.sig().fetch(&mut self.proc, cursor, 0, 8);
        let more = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Compare(Condition::Below),
            dst: more,
            lhs: pos,
            rhs: span.2,
        });
        self.proc.emit(Step::Branch {
            cond: more,
            truth: true,
            target: body,
        });
        self.proc.emit(Step::Store {
            base: exit.0,
            offset: 0,
            src: exit.1,
            width: Width::Quad,
        });
        self.proc.emit(Step::Jump(exit.2));
        self.proc.emit(Step::Mark(body));
        let la = self.advance(span.0, pos);
        let ra = self.advance(span.1, pos);
        let lb = self.byte(la);
        let rb = self.byte(ra);
        let eq = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Compare(Condition::Equal),
            dst: eq,
            lhs: lb,
            rhs: rb,
        });
        self.proc.emit(Step::Branch {
            cond: eq,
            truth: false,
            target: exit.2,
        });
        let one = Signature::constant(&mut self.proc, 1);
        let next = self.advance(pos, one);
        self.proc.emit(Step::Store {
            base: cursor,
            offset: 0,
            src: next,
            width: Width::Quad,
        });
        self.proc.emit(Step::Jump(head));
    }

    fn byte(&mut self, addr: u32) -> u32 {
        let dst = self.proc.value();
        self.proc.emit(Step::Load {
            dst,
            base: addr,
            offset: 0,
            access: Access {
                width: Width::Byte,
                signed: false,
            },
        });
        dst
    }
}
