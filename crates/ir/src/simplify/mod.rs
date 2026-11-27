mod fold;
mod prune;

use self::fold::evaluate;
use self::prune::{compact, sweep};
use crate::{Function, Instruction, Module, Operation, Target, Terminator, Type};

pub fn run(module: &mut Module) {
    for function in &mut module.functions {
        optimize(function);
    }
}

struct Folder {
    types: Vec<Option<Type>>,
    constants: Vec<Option<(i128, Type)>>,
    pairs: Vec<Option<(u32, u32)>>,
    aliases: Vec<u32>,
}

fn optimize(function: &mut Function) {
    if function.blocks.is_empty() {
        return;
    }
    let cap = (function.values as usize).max(1);
    let mut folder = Folder::new(cap);
    for (id, kind) in &function.inputs {
        folder.note(*id, kind.clone());
    }
    for block in &function.blocks {
        for (id, kind) in &block.parameters {
            folder.note(*id, kind.clone());
        }
    }
    for block in &mut function.blocks {
        for inst in &mut block.body {
            folder.rewrite(inst);
        }
        folder.ending(&mut block.exit);
    }
    for block in &mut function.blocks {
        for inst in &mut block.body {
            folder.remap(&mut inst.op);
        }
        folder.ending(&mut block.exit);
    }
    compact(&mut function.blocks);
    sweep(&mut function.blocks, cap);
}

impl Folder {
    fn new(limit: usize) -> Self {
        Self {
            types: vec![None; limit],
            constants: vec![None; limit],
            pairs: vec![None; limit],
            aliases: (0..limit as u32).collect(),
        }
    }

    fn resolve(&self, mut id: u32) -> u32 {
        while let Some(&next) = self.aliases.get(id as usize) {
            if next == id {
                break;
            }
            id = next;
        }
        id
    }

    fn note(&mut self, id: u32, kind: Type) {
        if let Some(slot) = self.types.get_mut(id as usize) {
            *slot = Some(kind);
        }
    }

    fn rewrite(&mut self, inst: &mut Instruction) {
        self.remap(&mut inst.op);
        if inst.kind != Type::Void {
            self.note(inst.value, inst.kind.clone());
        }
        let idx = inst.value as usize;
        if idx >= self.aliases.len() {
            return;
        }
        if let Some((num, kind)) = evaluate(&self.constants, &inst.op, &inst.kind) {
            inst.op = Operation::Integer {
                value: num,
                kind: kind.clone(),
            };
            self.constants[idx] = Some((num, kind));
            return;
        }
        match &inst.op {
            Operation::Pair { low, high } => self.pairs[idx] = Some((*low, *high)),
            Operation::Offset { base, bytes: 0 } => self.aliases[idx] = *base,
            Operation::Cast {
                value, from, kind, ..
            } if from == kind => self.aliases[idx] = *value,
            Operation::Low { pair, kind } => self.project(idx, (*pair, true), kind),
            Operation::High { pair, kind } => self.project(idx, (*pair, false), kind),
            _ => {}
        }
    }

    fn project(&mut self, dst: usize, src: (u32, bool), kind: &Type) {
        let Some(Some((low, high))) = self.pairs.get(src.0 as usize).copied() else {
            return;
        };
        let chosen = match src.1 {
            true => low,
            false => high,
        };
        if self.types.get(chosen as usize).and_then(Option::as_ref) == Some(kind) {
            self.aliases[dst] = chosen;
        }
    }

    fn remap(&self, op: &mut Operation) {
        match op {
            Operation::Unary { value, .. } | Operation::Cast { value, .. } => {
                *value = self.resolve(*value)
            }
            Operation::Binary { left, right, .. } => {
                *left = self.resolve(*left);
                *right = self.resolve(*right);
            }
            Operation::Pair { low, high } => {
                *low = self.resolve(*low);
                *high = self.resolve(*high);
            }
            Operation::Low { pair, .. } | Operation::High { pair, .. } => {
                *pair = self.resolve(*pair)
            }
            Operation::Offset { base, .. } => *base = self.resolve(*base),
            Operation::Load { address, .. } | Operation::Acquire { address, .. } => {
                *address = self.resolve(*address)
            }
            Operation::Store { address, value, .. } | Operation::Release { address, value, .. } => {
                *address = self.resolve(*address);
                *value = self.resolve(*value);
            }
            Operation::Copy {
                destination,
                source,
                ..
            } => {
                *destination = self.resolve(*destination);
                *source = self.resolve(*source);
            }
            other => self.extra(other),
        }
    }

    fn extra(&self, op: &mut Operation) {
        match op {
            Operation::Swap {
                address,
                expected,
                desired,
                ..
            } => {
                *address = self.resolve(*address);
                *expected = self.resolve(*expected);
                *desired = self.resolve(*desired);
            }
            Operation::Atomic {
                address, operand, ..
            } => {
                *address = self.resolve(*address);
                *operand = self.resolve(*operand);
            }
            Operation::Call { arguments, .. } | Operation::Spawn { arguments, .. } => {
                for item in arguments {
                    *item = self.resolve(*item);
                }
            }
            Operation::Indirect {
                callee, arguments, ..
            } => {
                *callee = self.resolve(*callee);
                for item in arguments {
                    *item = self.resolve(*item);
                }
            }
            Operation::Await { fiber, .. } => *fiber = self.resolve(*fiber),
            Operation::All { fibers, count, .. } | Operation::Race { fibers, count, .. } => {
                *fibers = self.resolve(*fibers);
                *count = self.resolve(*count);
            }
            _ => {}
        }
    }

    fn ending(&self, exit: &mut Terminator) {
        match exit {
            Terminator::Jump(item) => self.edge(item),
            Terminator::Branch { .. } => self.branch(exit),
            Terminator::Switch { .. } => self.select(exit),
            Terminator::Return(Some(val)) => *val = self.resolve(*val),
            Terminator::Return(None) | Terminator::Trap { .. } => {}
        }
    }

    fn branch(&self, exit: &mut Terminator) {
        let Terminator::Branch {
            condition,
            truthy,
            falsy,
        } = exit
        else {
            return;
        };
        *condition = self.resolve(*condition);
        self.edge(truthy);
        self.edge(falsy);
        if let Some(Some((num, _))) = self.constants.get(*condition as usize) {
            let chosen = match *num != 0 {
                true => truthy.clone(),
                false => falsy.clone(),
            };
            *exit = Terminator::Jump(chosen);
        }
    }

    fn select(&self, exit: &mut Terminator) {
        let Terminator::Switch {
            value,
            cases,
            default,
        } = exit
        else {
            return;
        };
        *value = self.resolve(*value);
        for (_, item) in cases.iter_mut() {
            self.edge(item);
        }
        self.edge(default);
        let Some(Some((num, Type::Integer { bits, .. }))) = self.constants.get(*value as usize)
        else {
            return;
        };
        let mask = match *bits >= 64 {
            true => u64::MAX,
            false => (1u64 << *bits) - 1,
        };
        let key = (*num as u64) & mask;
        let chosen = cases
            .iter()
            .find(|(tag, _)| *tag == key)
            .map_or_else(|| default.clone(), |(_, item)| item.clone());
        *exit = Terminator::Jump(chosen);
    }

    fn edge(&self, item: &mut Target) {
        for arg in &mut item.arguments {
            *arg = self.resolve(*arg);
        }
    }
}
