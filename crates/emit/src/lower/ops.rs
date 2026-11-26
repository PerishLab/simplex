use super::Function;
use super::abi::{Signature, Value, Write};
use crate::Fault;
use crate::encode::media::{Arithmetic, Precision};
use crate::encode::{Condition, Shift};
use crate::machine::linear::{Binary, Step, Unary};
use simplex_hir::{Expression, Type};

impl Function<'_, '_> {
    pub fn unary(
        &mut self,
        operator: &str,
        operand: &Expression,
        kind: &Type,
    ) -> Result<Value, Fault> {
        if operator == "*" {
            let ptr = self.scalar(operand)?;
            return Ok(self.sig().load(&mut self.proc, ptr, kind));
        }
        let src = self.scalar(operand)?;
        let dst = self.proc.value();
        let op = match (operator, kind) {
            ("-", Type::Float(w)) => Unary::Flip(Precision::from(*w)),
            ("-", _) => Unary::Negate,
            ("!", Type::Bool) => Unary::Not,
            ("!" | "~", _) => Unary::Invert,
            other => return Err(Fault::Unimplemented(format!("unary {}", other.0))),
        };
        self.proc.emit(Step::Unary { kind: op, dst, src });
        Ok(Value::Scalar(Signature::normalize(
            &mut self.proc,
            dst,
            kind,
        )))
    }

    pub fn binary(
        &mut self,
        operator: &str,
        pair: (&Expression, &Expression),
        kind: &Type,
    ) -> Result<Value, Fault> {
        match operator {
            "&&" => return self.logical(pair, false),
            "||" => return self.logical(pair, true),
            _ => {}
        }
        let domain = self.classify(pair.0);
        if let Type::Slice(elem) | Type::List(elem) = &domain {
            return self.slices(operator, pair, elem);
        }
        if let Type::Float(w) = domain {
            return self.float(operator, pair, Precision::from(w));
        }
        let lr = self.scalar(pair.0)?;
        let rr = self.scalar(pair.1)?;
        let lhs = Signature::normalize(&mut self.proc, lr, &domain);
        let rhs = Signature::normalize(&mut self.proc, rr, &domain);
        let signed = matches!(domain, Type::Signed(_));
        let op = integer(operator, signed)?;
        let dst = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: op,
            dst,
            lhs,
            rhs,
        });
        Ok(Value::Scalar(Signature::normalize(
            &mut self.proc,
            dst,
            kind,
        )))
    }

    fn logical(
        &mut self,
        pair: (&Expression, &Expression),
        disjunct: bool,
    ) -> Result<Value, Fault> {
        let slot = self.proc.reserve(8, 8);
        let base = Signature::base(&mut self.proc, slot, 0);
        let init = Signature::constant(&mut self.proc, i64::from(disjunct));
        self.sig().write(
            &mut self.proc,
            Write {
                ptr: base,
                offset: 0,
                bytes: 1,
            },
            init,
        );
        let done = self.proc.label();
        let lhs = self.scalar(pair.0)?;
        self.proc.emit(Step::Branch {
            cond: lhs,
            truth: disjunct,
            target: done,
        });
        let rr = self.scalar(pair.1)?;
        let rhs = Signature::normalize(&mut self.proc, rr, &Type::Bool);
        self.sig().write(
            &mut self.proc,
            Write {
                ptr: base,
                offset: 0,
                bytes: 1,
            },
            rhs,
        );
        self.proc.emit(Step::Mark(done));
        Ok(Value::Scalar(self.sig().fetch(&mut self.proc, base, 0, 1)))
    }

    fn float(
        &mut self,
        operator: &str,
        pair: (&Expression, &Expression),
        prec: Precision,
    ) -> Result<Value, Fault> {
        let lhs = self.scalar(pair.0)?;
        let rhs = self.scalar(pair.1)?;
        let dst = self.proc.value();
        let kind = match operator {
            "+" => Binary::Float(Arithmetic::Add, prec),
            "-" => Binary::Float(Arithmetic::Sub, prec),
            "*" => Binary::Float(Arithmetic::Mul, prec),
            "/" => Binary::Float(Arithmetic::Div, prec),
            "==" => Binary::Order("==", prec),
            "!=" => Binary::Order("!=", prec),
            "<" => Binary::Order("<", prec),
            "<=" => Binary::Order("<=", prec),
            ">" => Binary::Order(">", prec),
            ">=" => Binary::Order(">=", prec),
            other => return Err(Fault::Unimplemented(format!("float op {other}"))),
        };
        self.proc.emit(Step::Binary {
            kind,
            dst,
            lhs,
            rhs,
        });
        Ok(Value::Scalar(dst))
    }

    pub fn cast(&mut self, value: &Expression, target: &Type) -> Result<Value, Fault> {
        let source = self.classify(value);
        if matches!((&source, target), (Type::List(_), Type::Slice(_))) {
            return self.expression(value);
        }
        let raw = self.scalar(value)?;
        let norm = Signature::normalize(&mut self.proc, raw, &source);
        let dst = match (&source, target) {
            (Type::Float(a), Type::Float(b)) if a == b => norm,
            (Type::Float(_), Type::Float(b)) => {
                self.convert(norm, Unary::Resize(Precision::from(*b)))
            }
            (Type::Signed(_), Type::Float(b)) => {
                self.convert(norm, Unary::Promote(Precision::from(*b)))
            }
            (Type::Unsigned(_) | Type::Bool, Type::Float(b)) => {
                self.convert(norm, Unary::Unsigned(Precision::from(*b)))
            }
            (Type::Float(a), _) => {
                let int = self.convert(norm, Unary::Truncate(Precision::from(*a)));
                Signature::normalize(&mut self.proc, int, target)
            }
            _ => Signature::normalize(&mut self.proc, norm, target),
        };
        Ok(Value::Scalar(dst))
    }

    fn convert(&mut self, src: u32, kind: Unary) -> u32 {
        let dst = self.proc.value();
        self.proc.emit(Step::Unary { kind, dst, src });
        dst
    }
}

fn integer(operator: &str, signed: bool) -> Result<Binary, Fault> {
    let kind = match operator {
        "+" => Binary::Add,
        "-" => Binary::Sub,
        "*" => Binary::Mul,
        "/" => Binary::Quotient(signed),
        "%" => Binary::Remainder(signed),
        "&" => Binary::Conjunction,
        "|" => Binary::Disjunction,
        "^" => Binary::Exclusive,
        "<<" => Binary::Shift(Shift::Left),
        ">>" if signed => Binary::Shift(Shift::Arithmetic),
        ">>" => Binary::Shift(Shift::Logical),
        "==" => Binary::Compare(Condition::Equal),
        "!=" => Binary::Compare(Condition::Distinct),
        "<" if signed => Binary::Compare(Condition::Less),
        "<" => Binary::Compare(Condition::Below),
        "<=" if signed => Binary::Compare(Condition::Most),
        "<=" => Binary::Compare(Condition::Under),
        ">" if signed => Binary::Compare(Condition::Greater),
        ">" => Binary::Compare(Condition::Above),
        ">=" if signed => Binary::Compare(Condition::Least),
        ">=" => Binary::Compare(Condition::Over),
        other => return Err(Fault::Unimplemented(format!("binary op {other}"))),
    };
    Ok(kind)
}
