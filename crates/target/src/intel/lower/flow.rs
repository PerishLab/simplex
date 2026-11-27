use super::shape::Planner;
use crate::machine::Span;
use simplex_ir::{Block, Function, Operation, Target, Terminator};
use std::collections::{BTreeMap, BTreeSet};

struct Graph<'a> {
    func: &'a Function,
    planner: &'a Planner,
    hidden: Option<u32>,
    order: &'a [usize],
}

struct Sets {
    defs: Vec<BTreeSet<u32>>,
    uses: Vec<BTreeSet<u32>>,
    ins: Vec<BTreeSet<u32>>,
    outs: Vec<BTreeSet<u32>>,
}

pub(crate) fn successors(exit: &Terminator) -> Vec<&Target> {
    match exit {
        Terminator::Jump(target) => vec![target],
        Terminator::Branch { truthy, falsy, .. } => vec![truthy, falsy],
        Terminator::Switch { cases, default, .. } => {
            cases.iter().map(|(_, t)| t).chain([default]).collect()
        }
        Terminator::Return(_) | Terminator::Trap { .. } => Vec::new(),
    }
}

pub(crate) fn liveness(
    func: &Function,
    planner: &Planner,
    hidden: Option<u32>,
    ordered: &[usize],
) -> Vec<Span> {
    let graph = Graph {
        func,
        planner,
        hidden,
        order: ordered,
    };
    let mut sets = graph.summary();
    graph.solve(&mut sets);
    graph.intervals(&sets)
}

impl Graph<'_> {
    fn summary(&self) -> Sets {
        let count = self.order.len();
        let mut defs = vec![BTreeSet::new(); count];
        let mut uses = vec![BTreeSet::new(); count];
        for (p, &idx) in self.order.iter().enumerate() {
            self.local(&self.func.blocks[idx], &mut defs[p], &mut uses[p]);
        }
        Sets {
            defs,
            uses,
            ins: vec![BTreeSet::new(); count],
            outs: vec![BTreeSet::new(); count],
        }
    }

    fn local(&self, block: &Block, defs: &mut BTreeSet<u32>, uses: &mut BTreeSet<u32>) {
        let mut buf = Vec::new();
        for &(id, _) in &block.parameters {
            self.planner.regs(id, &mut buf);
        }
        defs.extend(buf.drain(..));
        for inst in &block.body {
            operands(&inst.op, self.planner, &mut buf);
            uses.extend(buf.drain(..).filter(|r| !defs.contains(r)));
            self.planner.regs(inst.value, &mut buf);
            defs.extend(buf.drain(..));
        }
        exits(&block.exit, self.planner, self.hidden, &mut buf);
        uses.extend(buf.into_iter().filter(|r| !defs.contains(r)));
    }

    fn solve(&self, sets: &mut Sets) {
        let pos: BTreeMap<u32, usize> = self
            .order
            .iter()
            .enumerate()
            .map(|(p, &i)| (self.func.blocks[i].id, p))
            .collect();
        let mut changed = true;
        while changed {
            changed = false;
            for p in (0..self.order.len()).rev() {
                changed |= self.step(p, &pos, sets);
            }
        }
    }

    fn step(&self, p: usize, pos: &BTreeMap<u32, usize>, sets: &mut Sets) -> bool {
        let mut out = BTreeSet::new();
        for succ in successors(&self.func.blocks[self.order[p]].exit) {
            if let Some(&sp) = pos.get(&succ.block) {
                out.extend(sets.ins[sp].iter().copied());
            }
        }
        let mut next = sets.uses[p].clone();
        next.extend(out.iter().copied().filter(|r| !sets.defs[p].contains(r)));
        if out != sets.outs[p] || next != sets.ins[p] {
            sets.outs[p] = out;
            sets.ins[p] = next;
            return true;
        }
        false
    }

    fn intervals(&self, sets: &Sets) -> Vec<Span> {
        let mut bounds: Vec<Option<(u32, u32)>> = vec![None; self.planner.floats.len()];
        let mut calls = Vec::new();
        let mut buf: Vec<u32> = self.hidden.into_iter().collect();
        for &(id, _) in &self.func.inputs {
            self.planner.regs(id, &mut buf);
        }
        touch(&mut bounds, &mut buf, 0);
        let mut point = 1u32;
        for (p, &idx) in self.order.iter().enumerate() {
            let block = &self.func.blocks[idx];
            buf.extend(sets.ins[p].iter().copied());
            for &(id, _) in &block.parameters {
                self.planner.regs(id, &mut buf);
            }
            touch(&mut bounds, &mut buf, point);
            point += 1;
            for inst in &block.body {
                if matches!(inst.op, Operation::Call { .. } | Operation::Indirect { .. }) {
                    calls.push(point);
                }
                operands(&inst.op, self.planner, &mut buf);
                self.planner.regs(inst.value, &mut buf);
                touch(&mut bounds, &mut buf, point);
                point += 1;
            }
            exits(&block.exit, self.planner, self.hidden, &mut buf);
            buf.extend(sets.outs[p].iter().copied());
            touch(&mut bounds, &mut buf, point);
            point += 1;
        }
        collect(&bounds, &self.planner.floats, &calls)
    }
}

fn collect(bounds: &[Option<(u32, u32)>], floats: &[bool], calls: &[u32]) -> Vec<Span> {
    bounds
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let (start, end) = (*item)?;
            let across = calls.iter().any(|&c| start < c && end > c);
            Some(Span {
                value: idx as u32,
                start,
                end,
                float: floats[idx],
                across,
            })
        })
        .collect()
}

fn touch(bounds: &mut [Option<(u32, u32)>], buf: &mut Vec<u32>, point: u32) {
    for reg in buf.drain(..) {
        let slot = &mut bounds[reg as usize];
        *slot = Some(slot.map_or((point, point), |(s, e)| (s.min(point), e.max(point))));
    }
}

fn operands(op: &Operation, planner: &Planner, out: &mut Vec<u32>) {
    match op {
        Operation::Integer { .. }
        | Operation::Float { .. }
        | Operation::Symbol { .. }
        | Operation::Slot { .. }
        | Operation::Fence { .. }
        | Operation::Yield => {}
        Operation::Unary { value, .. }
        | Operation::Cast { value, .. }
        | Operation::Low { pair: value, .. }
        | Operation::High { pair: value, .. }
        | Operation::Offset { base: value, .. }
        | Operation::Load { address: value, .. }
        | Operation::Acquire { address: value, .. }
        | Operation::Await { fiber: value, .. }
        | Operation::All { fibers: value, .. }
        | Operation::Race { fibers: value, .. } => planner.regs(*value, out),
        Operation::Binary { left, right, .. }
        | Operation::Pair {
            low: left,
            high: right,
        }
        | Operation::Store {
            address: left,
            value: right,
            ..
        }
        | Operation::Copy {
            destination: left,
            source: right,
            ..
        }
        | Operation::Release {
            address: left,
            value: right,
            ..
        }
        | Operation::Atomic {
            address: left,
            operand: right,
            ..
        } => {
            planner.regs(*left, out);
            planner.regs(*right, out);
        }
        Operation::Swap {
            address,
            expected,
            desired,
            ..
        } => {
            planner.regs(*address, out);
            planner.regs(*expected, out);
            planner.regs(*desired, out);
        }
        Operation::Call { arguments, .. } | Operation::Spawn { arguments, .. } => {
            arguments.iter().for_each(|&arg| planner.regs(arg, out));
        }
        Operation::Indirect {
            callee, arguments, ..
        } => {
            planner.regs(*callee, out);
            arguments.iter().for_each(|&arg| planner.regs(arg, out));
        }
    }
}

fn target(edge: &Target, planner: &Planner, out: &mut Vec<u32>) {
    edge.arguments
        .iter()
        .for_each(|&arg| planner.regs(arg, out));
}

fn exits(exit: &Terminator, planner: &Planner, hidden: Option<u32>, out: &mut Vec<u32>) {
    match exit {
        Terminator::Jump(edge) => target(edge, planner, out),
        Terminator::Branch {
            condition,
            truthy,
            falsy,
        } => {
            planner.regs(*condition, out);
            target(truthy, planner, out);
            target(falsy, planner, out);
        }
        Terminator::Switch {
            value,
            cases,
            default,
        } => {
            planner.regs(*value, out);
            cases
                .iter()
                .for_each(|(_, edge)| target(edge, planner, out));
            target(default, planner, out);
        }
        Terminator::Return(val) => {
            if let Some(v) = val {
                planner.regs(*v, out);
            }
            out.extend(hidden);
        }
        Terminator::Trap { .. } => {}
    }
}
