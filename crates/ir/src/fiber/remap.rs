use super::step::Worker;
use crate::{Operation, Type};
use std::collections::HashMap;

impl Worker<'_> {
    pub(super) fn remap(&mut self, op: &Operation, locals: &mut HashMap<u32, u32>) -> Operation {
        match op {
            Operation::Integer { value, kind } => Operation::Integer {
                value: *value,
                kind: kind.clone(),
            },
            Operation::Float { bits, kind } => Operation::Float {
                bits: *bits,
                kind: kind.clone(),
            },
            Operation::Symbol { name } => Operation::Symbol { name: name.clone() },
            Operation::Unary { op: u, value, kind } => Operation::Unary {
                op: *u,
                value: self.fetch(*value, locals),
                kind: kind.clone(),
            },
            Operation::Binary {
                op: bin,
                left,
                right,
                kind,
            } => Operation::Binary {
                op: *bin,
                left: self.fetch(*left, locals),
                right: self.fetch(*right, locals),
                kind: kind.clone(),
            },
            Operation::Cast {
                op: c,
                value,
                from,
                kind,
            } => Operation::Cast {
                op: *c,
                value: self.fetch(*value, locals),
                from: from.clone(),
                kind: kind.clone(),
            },
            Operation::Pair { low, high } => Operation::Pair {
                low: self.fetch(*low, locals),
                high: self.fetch(*high, locals),
            },
            Operation::Low { pair, kind } => Operation::Low {
                pair: self.fetch(*pair, locals),
                kind: kind.clone(),
            },
            Operation::High { pair, kind } => Operation::High {
                pair: self.fetch(*pair, locals),
                kind: kind.clone(),
            },
            Operation::Offset { base, bytes } => Operation::Offset {
                base: self.fetch(*base, locals),
                bytes: *bytes,
            },
            other => self.more(other, locals),
        }
    }

    fn more(&mut self, op: &Operation, locals: &mut HashMap<u32, u32>) -> Operation {
        match op {
            Operation::Load {
                address,
                kind,
                volatile,
            } => Operation::Load {
                address: self.fetch(*address, locals),
                kind: kind.clone(),
                volatile: *volatile,
            },
            Operation::Store {
                address,
                value,
                kind,
                volatile,
            } => Operation::Store {
                address: self.fetch(*address, locals),
                value: self.fetch(*value, locals),
                kind: kind.clone(),
                volatile: *volatile,
            },
            Operation::Copy {
                destination,
                source,
                bytes,
            } => Operation::Copy {
                destination: self.fetch(*destination, locals),
                source: self.fetch(*source, locals),
                bytes: *bytes,
            },
            Operation::Acquire {
                address,
                kind,
                order,
            } => Operation::Acquire {
                address: self.fetch(*address, locals),
                kind: kind.clone(),
                order: *order,
            },
            Operation::Release {
                address,
                value,
                kind,
                order,
            } => Operation::Release {
                address: self.fetch(*address, locals),
                value: self.fetch(*value, locals),
                kind: kind.clone(),
                order: *order,
            },
            Operation::Swap {
                address,
                expected,
                desired,
                kind,
                order,
            } => Operation::Swap {
                address: self.fetch(*address, locals),
                expected: self.fetch(*expected, locals),
                desired: self.fetch(*desired, locals),
                kind: kind.clone(),
                order: *order,
            },
            Operation::Atomic {
                op: a,
                address,
                operand,
                kind,
                order,
            } => Operation::Atomic {
                op: *a,
                address: self.fetch(*address, locals),
                operand: self.fetch(*operand, locals),
                kind: kind.clone(),
                order: *order,
            },
            Operation::Fence { order } => Operation::Fence { order: *order },
            Operation::Call {
                symbol,
                arguments,
                inputs,
                kind,
            } => Operation::Call {
                symbol: symbol.clone(),
                arguments: arguments.iter().map(|&id| self.fetch(id, locals)).collect(),
                inputs: inputs.clone(),
                kind: kind.clone(),
            },
            Operation::Spawn {
                symbol,
                arguments,
                inputs,
                ..
            } => Operation::Call {
                symbol: symbol.clone(),
                arguments: arguments.iter().map(|&id| self.fetch(id, locals)).collect(),
                inputs: inputs.clone(),
                kind: Type::Pointer,
            },
            Operation::Indirect {
                callee,
                arguments,
                inputs,
                kind,
            } => Operation::Indirect {
                callee: self.fetch(*callee, locals),
                arguments: arguments.iter().map(|&id| self.fetch(id, locals)).collect(),
                inputs: inputs.clone(),
                kind: kind.clone(),
            },
            _ => Operation::Fence {
                order: crate::Order::Relaxed,
            },
        }
    }
}
