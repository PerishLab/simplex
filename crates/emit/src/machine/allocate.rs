use super::linear::{Callee, Ending, Invocation, Outgoing, Parameter, Procedure, Step};
use crate::Register;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Register(Register),
    Spill(i32),
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub locations: Vec<Location>,
    pub bases: Vec<i32>,
    pub preserved: Vec<Register>,
    pub reserve: i32,
}

#[derive(Clone, Copy)]
struct Interval {
    value: u32,
    start: usize,
    end: usize,
    across: bool,
}

const VOLATILE: [Register; 9] = [
    Register::Rdi,
    Register::Rsi,
    Register::R8,
    Register::R9,
    Register::Rbx,
    Register::R12,
    Register::R13,
    Register::R14,
    Register::R15,
];

struct Tracker {
    spans: Vec<Option<(usize, usize)>>,
    marks: BTreeMap<u32, usize>,
    edges: Vec<(usize, usize)>,
    calls: Vec<usize>,
    outgoing: u32,
}

impl Tracker {
    fn new(count: usize) -> Self {
        Self {
            spans: vec![None; count],
            marks: BTreeMap::new(),
            edges: Vec::new(),
            calls: Vec::new(),
            outgoing: 0,
        }
    }

    fn touch(&mut self, value: u32, point: usize) {
        let slot = &mut self.spans[value as usize];
        *slot = Some(slot.map_or((point, point), |(s, e)| (s.min(point), e.max(point))));
    }

    fn observe(&mut self, point: usize, step: &Step) {
        match step {
            Step::Mark(label) => {
                self.marks.insert(label.0, point);
            }
            Step::Constant { dst, .. }
            | Step::Slot { dst, .. }
            | Step::Symbol { dst, .. }
            | Step::Zero { dst, .. } => self.touch(*dst, point),
            Step::Move { dst, src }
            | Step::Unary { dst, src, .. }
            | Step::Load { dst, base: src, .. }
            | Step::Store { base: dst, src, .. }
            | Step::Copy { dst, src, .. } => {
                self.touch(*src, point);
                self.touch(*dst, point);
            }
            Step::Binary { dst, lhs, rhs, .. } => {
                self.touch(*lhs, point);
                self.touch(*rhs, point);
                self.touch(*dst, point);
            }
            Step::Jump(target) => self.edge(target.0, point),
            Step::Branch { cond, target, .. } => {
                self.touch(*cond, point);
                self.edge(target.0, point);
            }
            Step::Invoke(call) => self.call(point, call),
            Step::Finish(ending) => self.ending(point, *ending),
        }
    }

    fn edge(&mut self, label: u32, point: usize) {
        if let Some(&header) = self.marks.get(&label)
            && header <= point
        {
            self.edges.push((header, point));
        }
    }

    fn call(&mut self, point: usize, call: &Invocation) {
        self.calls.push(point);
        self.outgoing = self.outgoing.max(call.space);
        if let Callee::Indirect(reg) = call.callee {
            self.touch(reg, point);
        }
        for &arg in &call.registers {
            self.touch(arg, point);
        }
        for &(arg, _) in &call.floats {
            self.touch(arg, point);
        }
        for item in &call.stack {
            let reg = match *item {
                Outgoing::Scalar { src, .. } => src,
                Outgoing::Block { base, .. } => base,
            };
            self.touch(reg, point);
        }
        for out in [call.first, call.second].into_iter().flatten() {
            self.touch(out, point);
        }
    }

    fn ending(&mut self, point: usize, ending: Ending) {
        match ending {
            Ending::Void => {}
            Ending::Single(v) => self.touch(v, point),
            Ending::Pair(a, b) => self.pair(a, b, point),
            Ending::Indirect { target, source, .. } => self.pair(target, source, point),
        }
    }

    fn pair(&mut self, first: u32, second: u32, point: usize) {
        self.touch(first, point);
        self.touch(second, point);
    }

    fn loops(&mut self) {
        let mut changed = true;
        while changed {
            changed = false;
            for idx in 0..self.edges.len() {
                let (header, back) = self.edges[idx];
                changed |= self.stretch(header, back);
            }
        }
    }

    fn stretch(&mut self, header: usize, back: usize) -> bool {
        let mut updated = false;
        for span in self.spans.iter_mut().flatten() {
            if span.0 <= header && span.1 >= header && span.1 < back {
                span.1 = back;
                updated = true;
            }
        }
        updated
    }
}

pub fn allocate(proc: &Procedure) -> Frame {
    let mut tracker = Tracker::new(proc.values as usize);
    for param in &proc.parameters {
        if let Parameter::Register { dst, .. } | Parameter::Stack { dst, .. } = *param {
            tracker.touch(dst, 0);
        }
    }
    for (idx, step) in proc.steps.iter().enumerate() {
        tracker.observe(idx + 1, step);
    }
    tracker.loops();
    assign(proc, &build(&tracker), tracker.outgoing)
}

#[derive(Clone, Copy)]
struct Active {
    end: usize,
    value: u32,
    register: Register,
}

fn build(tracker: &Tracker) -> Vec<Interval> {
    let mut list: Vec<Interval> = tracker
        .spans
        .iter()
        .enumerate()
        .filter_map(|(idx, span)| {
            let (start, end) = (*span)?;
            let across = tracker.calls.iter().any(|&c| start < c && end > c);
            Some(Interval {
                value: idx as u32,
                start,
                end,
                across,
            })
        })
        .collect();
    list.sort_by_key(|item| (item.start, item.end));
    list
}

fn assign(proc: &Procedure, intervals: &[Interval], outgoing: u32) -> Frame {
    let count = proc.values as usize;
    let mut chosen: Vec<Option<Register>> = vec![None; count];
    let mut spilled = vec![false; count];
    let mut active: Vec<Active> = Vec::new();
    for item in intervals {
        active.retain(|entry| entry.end >= item.start);
        let pool: &[Register] = match item.across {
            true => &Register::PRESERVED,
            false => &VOLATILE,
        };
        let free = pool
            .iter()
            .copied()
            .find(|&reg| !active.iter().any(|entry| entry.register == reg));
        match free {
            Some(reg) => {
                chosen[item.value as usize] = Some(reg);
                active.push(Active {
                    end: item.end,
                    value: item.value,
                    register: reg,
                });
            }
            None => evict(item, &mut active, &mut chosen, &mut spilled),
        }
    }
    layout(proc, &chosen, &spilled, outgoing)
}

fn evict(
    item: &Interval,
    active: &mut Vec<Active>,
    chosen: &mut [Option<Register>],
    spilled: &mut [bool],
) {
    let candidate = active
        .iter()
        .enumerate()
        .filter(|(_, entry)| !item.across || Register::PRESERVED.contains(&entry.register))
        .max_by_key(|(_, entry)| entry.end);
    match candidate {
        Some((pos, entry)) if entry.end > item.end => {
            let victim = active.remove(pos);
            chosen[victim.value as usize] = None;
            spilled[victim.value as usize] = true;
            chosen[item.value as usize] = Some(victim.register);
            active.push(Active {
                end: item.end,
                value: item.value,
                register: victim.register,
            });
        }
        _ => spilled[item.value as usize] = true,
    }
}

fn layout(proc: &Procedure, chosen: &[Option<Register>], spilled: &[bool], outgoing: u32) -> Frame {
    let preserved: Vec<Register> = Register::PRESERVED
        .into_iter()
        .filter(|r| chosen.contains(&Some(*r)))
        .collect();
    let saved = (preserved.len() * 8) as i32;
    let mut cursor = saved;
    let mut bases = Vec::with_capacity(proc.slots.len());
    for slot in &proc.slots {
        let align = slot.align.max(8) as i32;
        let bytes = ((slot.bytes.max(8) + 7) & !7) as i32;
        cursor = (cursor + bytes + align - 1) & !(align - 1);
        bases.push(-cursor);
    }
    let mut locations = Vec::with_capacity(chosen.len());
    for idx in 0..chosen.len() {
        match chosen[idx] {
            Some(reg) => locations.push(Location::Register(reg)),
            None if spilled[idx] => {
                cursor += 8;
                locations.push(Location::Spill(-cursor));
            }
            None => locations.push(Location::Register(Register::R10)),
        }
    }
    let total = (cursor + outgoing as i32 + 15) & !15;
    Frame {
        locations,
        bases,
        preserved,
        reserve: total - saved,
    }
}
