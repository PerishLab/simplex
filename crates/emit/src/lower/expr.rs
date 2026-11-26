use super::Function;
use super::abi::{Signature, Value};
use crate::Fault;
use crate::machine::linear::{Callee, Step};
use simplex_hir::{Expression, Type};
use simplex_runtime::Hook;

impl Function<'_, '_> {
    pub fn expression(&mut self, expr: &Expression) -> Result<Value, Fault> {
        match expr {
            Expression::Void => Ok(Value::Void),
            Expression::Int(v, kind) => {
                let raw = Signature::constant(&mut self.proc, *v as i64);
                Ok(Value::Scalar(Signature::normalize(
                    &mut self.proc,
                    raw,
                    kind,
                )))
            }
            Expression::Float(bits, _) => Ok(Value::Scalar(Signature::constant(
                &mut self.proc,
                *bits as i64,
            ))),
            Expression::Bool(b) => Ok(Value::Scalar(Signature::constant(
                &mut self.proc,
                i64::from(*b),
            ))),
            Expression::Bytes(sym) => Ok(self.bytes(sym)),
            Expression::Symbol(sym, _) => {
                let external = self.unit.imports.iter().any(|i| i.symbol == *sym);
                let dst = self.proc.value();
                self.proc.emit(Step::Symbol {
                    dst,
                    name: sym.clone(),
                    external,
                });
                Ok(Value::Scalar(dst))
            }
            Expression::Local(name, _) if name == "continue" => {
                self.again()?;
                Ok(Value::Void)
            }
            Expression::Local(name, kind) => {
                let ptr = self.address(name)?;
                Ok(self.sig().load(&mut self.proc, ptr, kind))
            }
            Expression::Unary {
                operator,
                operand,
                kind,
            } => self.unary(operator, operand, kind),
            Expression::Binary {
                operator,
                left,
                right,
                kind,
            } => self.binary(operator, (left, right), kind),
            Expression::Cast { value, kind } => self.cast(value, kind),
            Expression::Borrow { target, kind, .. } => self.borrow(target, kind),
            Expression::Load { pointer, kind } => {
                let ptr = self.scalar(pointer)?;
                Ok(self.sig().load(&mut self.proc, ptr, kind))
            }
            Expression::Field { kind, .. } | Expression::Index { kind, .. } => {
                let ptr = self.place(expr)?;
                Ok(self.sig().load(&mut self.proc, ptr, kind))
            }
            Expression::Window {
                base,
                start,
                limit,
                kind,
            } => self.window(base, (start, limit), kind),
            Expression::Length(inner) => {
                let (_, len) = self.sequence(inner)?;
                Ok(Value::Scalar(len))
            }
            Expression::Record { name, fields, kind } => self.record(name, fields, kind),
            Expression::Variant {
                choice,
                arm,
                tag,
                payloads,
                kind,
            } => self.variant((choice, arm, *tag), payloads, kind),
            Expression::List { items, element, .. } => self.list(items, element),
            Expression::Buffer { count, element, .. } => self.buffer(count, element),
            Expression::Coerce { value, table, .. } => self.coerce(value, table),
            Expression::Call {
                symbol,
                arguments,
                kind,
            } => self.direct(symbol, arguments, kind),
            Expression::Virtual {
                receiver,
                slot,
                arguments,
                kind,
            } => self.dispatch((receiver, *slot), arguments, kind),
            Expression::Indirect {
                callee,
                arguments,
                kind,
            } => self.indirect(callee, arguments, kind),
            Expression::Block { body, tail, .. } => self.block(body, tail.as_deref()),
            Expression::Branch {
                condition,
                then,
                otherwise,
                kind,
            } => self.conditional(condition, (then, otherwise), kind),
            Expression::Switch {
                subject,
                cases,
                kind,
            } => self.select(subject, cases, kind),
            Expression::Loop { body, kind } => self.repeat(body, kind),
            Expression::Yield => {
                let callee = Callee::Direct {
                    symbol: Hook::Yield.symbol().to_owned(),
                    external: false,
                };
                let _ = self.sig().invoke(&mut self.proc, callee, &[], &Type::Void);
                Ok(Value::Void)
            }
            Expression::Await { fiber, kind } => {
                let handle = self.scalar(fiber)?;
                Ok(self.settle(Hook::Await, vec![handle], kind))
            }
            Expression::All {
                fibers, element, ..
            } => self.gather(fibers, element),
            Expression::Race { fibers, kind } => {
                let (fptr, count) = self.sequence(fibers)?;
                Ok(self.settle(Hook::Race, vec![fptr, count], kind))
            }
        }
    }

    fn bytes(&mut self, sym: &str) -> Value {
        if let Some(asset) = self.unit.assets.iter().find(|a| a.symbol == sym) {
            let ptr = self.proc.value();
            self.proc.emit(Step::Symbol {
                dst: ptr,
                name: asset.symbol.clone(),
                external: false,
            });
            let len = Signature::constant(&mut self.proc, asset.bytes.len() as i64);
            return Value::Pair(ptr, len);
        }
        let idx = self.literals.len();
        let name = format!("sx$lit${idx}");
        self.literals.push((name.clone(), sym.as_bytes().to_vec()));
        let ptr = self.proc.value();
        self.proc.emit(Step::Symbol {
            dst: ptr,
            name,
            external: false,
        });
        let len = Signature::constant(&mut self.proc, sym.len() as i64);
        Value::Pair(ptr, len)
    }

    fn borrow(&mut self, target: &Expression, kind: &Type) -> Result<Value, Fault> {
        let source = self.classify(target);
        match (&source, kind) {
            (Type::List(_), Type::Slice(_)) | (Type::Slice(_), Type::Slice(_)) => {
                self.expression(target)
            }
            (a, b) if a == b => self.expression(target),
            _ => Ok(Value::Scalar(self.place(target)?)),
        }
    }
}
