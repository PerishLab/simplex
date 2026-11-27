mod call;
mod data;
mod place;

use super::builder::Builder;
use super::types::{BOOL, WIDE, convert};
use crate::tree::Expression;
use simplex_ir::Operation;

impl Builder<'_, '_> {
    pub fn expression(&mut self, expr: &Expression) -> u32 {
        if self.sealed {
            return 0;
        }
        match expr {
            Expression::Void => 0,
            Expression::Int(value, kind) => {
                let lowered = convert(kind, &self.unit.catalog);
                self.integer(*value, lowered)
            }
            Expression::Float(bits, kind) => {
                let lowered = convert(kind, &self.unit.catalog);
                self.emit(
                    lowered.clone(),
                    Operation::Float {
                        bits: *bits,
                        kind: lowered,
                    },
                )
            }
            Expression::Bool(flag) => self.integer(i128::from(*flag), BOOL),
            Expression::Bytes(symbol) => self.bytes(symbol),
            Expression::Symbol(symbol, _) => self.emit(
                simplex_ir::Type::Pointer,
                Operation::Symbol {
                    name: symbol.clone(),
                },
            ),
            Expression::Local(name, _) if name == "continue" => {
                self.again();
                0
            }
            Expression::Local(name, _) if name == "break" => {
                self.escape(None);
                0
            }
            Expression::Local(_, kind)
            | Expression::Field { kind, .. }
            | Expression::Index { kind, .. } => {
                let address = self.place(expr);
                self.load(address, kind)
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
                let address = self.expression(pointer);
                self.load(address, kind)
            }
            Expression::Window {
                base,
                start,
                limit,
                kind,
            } => self.window(base, (start, limit), kind),
            Expression::Length(inner) => self.sequence(inner).1,
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
            Expression::Block { body, tail, .. } => self.scoped(body, tail.as_deref()),
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
            Expression::Yield
            | Expression::Await { .. }
            | Expression::All { .. }
            | Expression::Race { .. } => self.concurrent(expr),
        }
    }

    fn concurrent(&mut self, expr: &Expression) -> u32 {
        match expr {
            Expression::Yield => self.emit(simplex_ir::Type::Void, Operation::Yield),
            Expression::Await { fiber, kind } => {
                let handle = self.expression(fiber);
                let lowered = convert(kind, &self.unit.catalog);
                self.emit(
                    lowered.clone(),
                    Operation::Await {
                        fiber: handle,
                        kind: lowered,
                    },
                )
            }
            Expression::All {
                fibers, element, ..
            } => {
                let (pointer, count) = self.sequence(fibers);
                let lowered = convert(element, &self.unit.catalog);
                self.emit(
                    simplex_ir::Type::Pair,
                    Operation::All {
                        fibers: pointer,
                        count,
                        element: lowered,
                    },
                )
            }
            Expression::Race { fibers, kind } => {
                let (pointer, count) = self.sequence(fibers);
                let lowered = convert(kind, &self.unit.catalog);
                self.emit(
                    lowered.clone(),
                    Operation::Race {
                        fibers: pointer,
                        count,
                        kind: lowered,
                    },
                )
            }
            _ => 0,
        }
    }

    fn bytes(&mut self, symbol: &str) -> u32 {
        let (name, count) = self.vault.intern(symbol);
        let low = self.emit(simplex_ir::Type::Pointer, Operation::Symbol { name });
        let high = self.integer(count as i128, WIDE);
        self.emit(simplex_ir::Type::Pair, Operation::Pair { low, high })
    }

    fn borrow(&mut self, target: &Expression, kind: &crate::Type) -> u32 {
        let source = self.classify(target);
        match (&source, kind) {
            (crate::Type::List(_), crate::Type::Slice(_))
            | (crate::Type::Slice(_), crate::Type::Slice(_)) => self.expression(target),
            (left, right) if left == right => self.expression(target),
            _ => self.place(target),
        }
    }
}
