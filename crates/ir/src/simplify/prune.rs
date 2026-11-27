use crate::{Block, Operation, Terminator};
use std::collections::{HashMap, HashSet};

pub fn compact(blocks: &mut Vec<Block>) {
    let map: HashMap<u32, usize> = blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut seen = HashSet::with_capacity(blocks.len());
    let mut stack = vec![blocks[0].id];
    seen.insert(blocks[0].id);
    while let Some(id) = stack.pop() {
        if let Some(&idx) = map.get(&id) {
            enqueue(&blocks[idx].exit, &mut seen, &mut stack);
        }
    }
    blocks.retain(|block| seen.contains(&block.id));
}

fn enqueue(exit: &Terminator, seen: &mut HashSet<u32>, stack: &mut Vec<u32>) {
    match exit {
        Terminator::Jump(item) => push(item.block, seen, stack),
        Terminator::Branch { truthy, falsy, .. } => {
            push(truthy.block, seen, stack);
            push(falsy.block, seen, stack);
        }
        Terminator::Switch { cases, default, .. } => {
            for (_, item) in cases {
                push(item.block, seen, stack);
            }
            push(default.block, seen, stack);
        }
        Terminator::Return(_) | Terminator::Trap { .. } => {}
    }
}

fn push(id: u32, seen: &mut HashSet<u32>, stack: &mut Vec<u32>) {
    if seen.insert(id) {
        stack.push(id);
    }
}

pub fn sweep(blocks: &mut [Block], cap: usize) {
    let mut counts = vec![0u32; cap];
    let mut origin: Vec<Option<(usize, usize)>> = vec![None; cap];
    let mut refs = Vec::new();
    for (bidx, block) in blocks.iter().enumerate() {
        for (iidx, inst) in block.body.iter().enumerate() {
            if pure(&inst.op)
                && let Some(slot) = origin.get_mut(inst.value as usize)
            {
                *slot = Some((bidx, iidx));
            }
            refs.clear();
            trace(&inst.op, &mut refs);
            bump(&refs, &mut counts);
        }
        refs.clear();
        finish(&block.exit, &mut refs);
        bump(&refs, &mut counts);
    }
    let mut dead: Vec<usize> = (0..cap)
        .filter(|&id| origin[id].is_some() && counts[id] == 0)
        .collect();
    while let Some(id) = dead.pop() {
        if let Some((bidx, iidx)) = origin[id] {
            refs.clear();
            trace(&blocks[bidx].body[iidx].op, &mut refs);
            release(&refs, &origin, &mut counts, &mut dead);
        }
    }
    for block in blocks.iter_mut() {
        block.body.retain(|inst| {
            !pure(&inst.op) || counts.get(inst.value as usize).copied().unwrap_or(1) > 0
        });
    }
}

fn bump(refs: &[u32], counts: &mut [u32]) {
    for &id in refs {
        if let Some(slot) = counts.get_mut(id as usize) {
            *slot += 1;
        }
    }
}

fn release(
    refs: &[u32],
    origin: &[Option<(usize, usize)>],
    counts: &mut [u32],
    dead: &mut Vec<usize>,
) {
    for &id in refs {
        let idx = id as usize;
        if let Some(slot) = counts.get_mut(idx)
            && *slot > 0
        {
            *slot -= 1;
            if *slot == 0 && origin[idx].is_some() {
                dead.push(idx);
            }
        }
    }
}

fn pure(op: &Operation) -> bool {
    matches!(
        op,
        Operation::Integer { .. }
            | Operation::Float { .. }
            | Operation::Symbol { .. }
            | Operation::Unary { .. }
            | Operation::Binary { .. }
            | Operation::Cast { .. }
            | Operation::Pair { .. }
            | Operation::Low { .. }
            | Operation::High { .. }
            | Operation::Offset { .. }
    )
}

fn trace(op: &Operation, out: &mut Vec<u32>) {
    match op {
        Operation::Unary { value, .. } | Operation::Cast { value, .. } => out.push(*value),
        Operation::Binary { left, right, .. } => {
            out.push(*left);
            out.push(*right);
        }
        Operation::Pair { low, high } => {
            out.push(*low);
            out.push(*high);
        }
        Operation::Low { pair, .. } | Operation::High { pair, .. } => out.push(*pair),
        Operation::Offset { base, .. } => out.push(*base),
        Operation::Load { address, .. } | Operation::Acquire { address, .. } => out.push(*address),
        Operation::Store { address, value, .. } | Operation::Release { address, value, .. } => {
            out.push(*address);
            out.push(*value);
        }
        Operation::Copy {
            destination,
            source,
            ..
        } => {
            out.push(*destination);
            out.push(*source);
        }
        other => follow(other, out),
    }
}

fn follow(op: &Operation, out: &mut Vec<u32>) {
    match op {
        Operation::Swap {
            address,
            expected,
            desired,
            ..
        } => {
            out.push(*address);
            out.push(*expected);
            out.push(*desired);
        }
        Operation::Atomic {
            address, operand, ..
        } => {
            out.push(*address);
            out.push(*operand);
        }
        Operation::Call { arguments, .. } | Operation::Spawn { arguments, .. } => {
            out.extend_from_slice(arguments);
        }
        Operation::Indirect {
            callee, arguments, ..
        } => {
            out.push(*callee);
            out.extend_from_slice(arguments);
        }
        Operation::Await { fiber, .. } => out.push(*fiber),
        Operation::All { fibers, count, .. } | Operation::Race { fibers, count, .. } => {
            out.push(*fibers);
            out.push(*count);
        }
        _ => {}
    }
}

fn finish(exit: &Terminator, out: &mut Vec<u32>) {
    match exit {
        Terminator::Jump(item) => out.extend_from_slice(&item.arguments),
        Terminator::Branch {
            condition,
            truthy,
            falsy,
        } => {
            out.push(*condition);
            out.extend_from_slice(&truthy.arguments);
            out.extend_from_slice(&falsy.arguments);
        }
        Terminator::Switch {
            value,
            cases,
            default,
        } => {
            out.push(*value);
            for (_, item) in cases {
                out.extend_from_slice(&item.arguments);
            }
            out.extend_from_slice(&default.arguments);
        }
        Terminator::Return(Some(val)) => out.push(*val),
        Terminator::Return(None) | Terminator::Trap { .. } => {}
    }
}
