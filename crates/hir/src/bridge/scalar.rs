use super::builder::Builder;
use super::types::{BOOL, BYTE, WIDE, convert};
use crate::tree::Expression;
use simplex_ir::{Binary, Cast, Operation, Target, Terminator, Unary};

impl Builder<'_, '_> {
    pub fn unary(&mut self, operator: &str, operand: &Expression, kind: &crate::Type) -> u32 {
        if operator == "*" {
            let pointer = self.expression(operand);
            return self.load(pointer, kind);
        }
        let value = self.expression(operand);
        let lowered = convert(kind, &self.unit.catalog);
        match (operator, kind) {
            ("-", _) => self.emit(
                lowered.clone(),
                Operation::Unary {
                    op: Unary::Neg,
                    value,
                    kind: lowered,
                },
            ),
            ("!", crate::Type::Bool) => {
                let zero = self.integer(0, BOOL);
                self.emit(
                    BOOL,
                    Operation::Binary {
                        op: Binary::Eq,
                        left: value,
                        right: zero,
                        kind: BOOL,
                    },
                )
            }
            _ => self.emit(
                lowered.clone(),
                Operation::Unary {
                    op: Unary::Not,
                    value,
                    kind: lowered,
                },
            ),
        }
    }

    pub fn binary(
        &mut self,
        operator: &str,
        pair: (&Expression, &Expression),
        kind: &crate::Type,
    ) -> u32 {
        match operator {
            "&&" => return self.logical(pair, false),
            "||" => return self.logical(pair, true),
            _ => {}
        }
        let domain = self.classify(pair.0);
        if let crate::Type::Slice(elem) | crate::Type::List(elem) = &domain {
            return self.slices(operator, pair, elem);
        }
        if matches!(domain, crate::Type::Record(_) | crate::Type::Choice(_)) {
            return self.aggregates(operator, pair, &domain);
        }
        let left = self.expression(pair.0);
        let right = self.expression(pair.1);
        let source = self.classify(pair.1);
        let matched = self.recast(right, &source, &domain);
        let (lhs, rhs, operand) = match convert(&domain, &self.unit.catalog) {
            simplex_ir::Type::Pointer => (self.raw(left), self.raw(matched), WIDE),
            other => (left, matched, other),
        };
        let (op, comparison) = arithmetic(operator);
        let out = match comparison {
            true => BOOL,
            false => convert(kind, &self.unit.catalog),
        };
        self.emit(
            out,
            Operation::Binary {
                op,
                left: lhs,
                right: rhs,
                kind: operand,
            },
        )
    }

    fn raw(&mut self, pointer: u32) -> u32 {
        self.emit(
            WIDE,
            Operation::Cast {
                op: Cast::Bitcast,
                value: pointer,
                from: simplex_ir::Type::Pointer,
                kind: WIDE,
            },
        )
    }

    fn logical(&mut self, pair: (&Expression, &Expression), disjunct: bool) -> u32 {
        let first = self.expression(pair.0);
        if self.sealed {
            return first;
        }
        let short = self.integer(i128::from(disjunct), BOOL);
        let result = self.fresh();
        let next = self.block(Vec::new());
        let joined = self.block(vec![(result, BOOL)]);
        let (truthy, falsy) = match disjunct {
            true => (
                Target {
                    block: joined,
                    arguments: vec![short],
                },
                Target {
                    block: next,
                    arguments: Vec::new(),
                },
            ),
            false => (
                Target {
                    block: next,
                    arguments: Vec::new(),
                },
                Target {
                    block: joined,
                    arguments: vec![short],
                },
            ),
        };
        self.terminate(Terminator::Branch {
            condition: first,
            truthy,
            falsy,
        });
        self.activate(next, true);
        let second = self.expression(pair.1);
        if !self.sealed {
            self.terminate(Terminator::Jump(Target {
                block: joined,
                arguments: vec![second],
            }));
        }
        self.activate(joined, true);
        result
    }

    pub fn cast(&mut self, value: &Expression, target: &crate::Type) -> u32 {
        let source = self.classify(value);
        let raw = self.expression(value);
        self.recast(raw, &source, target)
    }

    pub fn recast(&mut self, raw: u32, source: &crate::Type, target: &crate::Type) -> u32 {
        let catalog = &self.unit.catalog;
        let from = convert(source, catalog);
        let into = convert(target, catalog);
        if from == into {
            return raw;
        }
        if from == simplex_ir::Type::Void || into == simplex_ir::Type::Void {
            return self.zero(target);
        }
        if let Some(bridged) = self.compound(raw, (source, target), (&from, &into)) {
            return bridged;
        }
        let op = conversion(&from, &into);
        self.emit(
            into.clone(),
            Operation::Cast {
                op,
                value: raw,
                from,
                kind: into,
            },
        )
    }

    fn compound(
        &mut self,
        raw: u32,
        kinds: (&crate::Type, &crate::Type),
        lowered: (&simplex_ir::Type, &simplex_ir::Type),
    ) -> Option<u32> {
        let (source, target) = kinds;
        match lowered {
            (simplex_ir::Type::Aggregate { .. }, _) => {
                let base = self.materialize(raw, source);
                let byte = self.emit(
                    BYTE,
                    Operation::Load {
                        address: base,
                        kind: BYTE,
                        volatile: false,
                    },
                );
                Some(self.recast(byte, &crate::Type::Unsigned(1), target))
            }
            (simplex_ir::Type::Pair, _) => {
                let low = self.emit(
                    WIDE,
                    Operation::Low {
                        pair: raw,
                        kind: WIDE,
                    },
                );
                Some(self.recast(low, &crate::Type::Unsigned(8), target))
            }
            (_, simplex_ir::Type::Aggregate { .. } | simplex_ir::Type::Pair) => {
                Some(self.zero(target))
            }
            (simplex_ir::Type::Pointer, simplex_ir::Type::Integer { bits, .. }) if *bits != 64 => {
                let wide = self.raw(raw);
                Some(self.recast(wide, &crate::Type::Unsigned(8), target))
            }
            (simplex_ir::Type::Integer { bits, .. }, simplex_ir::Type::Pointer) if *bits != 64 => {
                let wide = self.recast(raw, source, &crate::Type::Unsigned(8));
                Some(self.recast(wide, &crate::Type::Unsigned(8), target))
            }
            _ => None,
        }
    }
}

fn conversion(from: &simplex_ir::Type, into: &simplex_ir::Type) -> Cast {
    match (from, into) {
        (simplex_ir::Type::Integer { bits: a, .. }, simplex_ir::Type::Integer { bits: b, .. }) => {
            match a.cmp(b) {
                std::cmp::Ordering::Less => Cast::Widen,
                std::cmp::Ordering::Greater => Cast::Narrow,
                std::cmp::Ordering::Equal => Cast::Bitcast,
            }
        }
        (simplex_ir::Type::Integer { signed: true, .. }, simplex_ir::Type::Float { .. }) => {
            Cast::Promote
        }
        (simplex_ir::Type::Integer { signed: false, .. }, simplex_ir::Type::Float { .. }) => {
            Cast::Elevate
        }
        (simplex_ir::Type::Float { .. }, simplex_ir::Type::Integer { signed: true, .. }) => {
            Cast::Truncate
        }
        (simplex_ir::Type::Float { .. }, simplex_ir::Type::Integer { signed: false, .. }) => {
            Cast::Demote
        }
        (simplex_ir::Type::Float { bits: 32 }, simplex_ir::Type::Float { bits: 64 }) => {
            Cast::Extend
        }
        (simplex_ir::Type::Float { bits: 64 }, simplex_ir::Type::Float { bits: 32 }) => {
            Cast::Shrink
        }
        _ => Cast::Bitcast,
    }
}

fn arithmetic(operator: &str) -> (Binary, bool) {
    match operator {
        "+" => (Binary::Add, false),
        "-" => (Binary::Sub, false),
        "*" => (Binary::Mul, false),
        "/" => (Binary::Div, false),
        "%" => (Binary::Rem, false),
        "&" => (Binary::And, false),
        "|" => (Binary::Or, false),
        "^" => (Binary::Xor, false),
        "<<" => (Binary::Shl, false),
        ">>" => (Binary::Shr, false),
        "==" => (Binary::Eq, true),
        "!=" => (Binary::Ne, true),
        "<" => (Binary::Lt, true),
        "<=" => (Binary::Le, true),
        ">" => (Binary::Gt, true),
        _ => (Binary::Ge, true),
    }
}
