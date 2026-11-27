use crate::{Function, Instruction, Operation, Terminator, Type};
use std::collections::{HashMap, HashSet};

pub const STEP: i64 = 0x00;
pub const CANCEL: i64 = 0x08;
pub const CHILD: i64 = 0x10;
pub const STATE: i64 = 0x18;
pub const SIZE: i64 = 0x20;
pub const RESULT: i64 = 0x28;
pub const COMPLETED: u64 = u64::MAX;
pub const CANCELLED: u64 = u64::MAX - 1;

pub struct Plan {
    pub total: u64,
    pub batch: i64,
    pub count: i64,
    pub first: Option<(i64, u32)>,
    pub slots: HashMap<u32, i64>,
    pub values: HashMap<u32, i64>,
    pub types: HashMap<u32, Type>,
}

impl Plan {
    pub fn analyze(function: &Function) -> Self {
        let result = match function.output {
            Type::Aggregate { size, .. } => align(i64::from(size.max(16)), 8),
            _ => 16,
        };
        let batch = RESULT + result;
        let count = batch + 8;
        let mut cursor = count + 8;
        let mut slots = HashMap::new();
        let mut first = None;
        for (bidx, block) in function.blocks.iter().enumerate() {
            cursor = place(cursor, bidx == 0, &block.body, (&mut first, &mut slots));
        }
        let (types, spilled) = liveness(function, &slots);
        let mut values = HashMap::new();
        for &(id, ref kind) in &function.inputs {
            cursor = reserve(cursor, id, kind, &mut values);
        }
        for id in spilled {
            if !values.contains_key(&id)
                && let Some(kind) = types.get(&id)
            {
                cursor = reserve(cursor, id, kind, &mut values);
            }
        }
        let total = align(cursor, 16).max(64) as u64;
        Self {
            total,
            batch,
            count,
            first,
            slots,
            values,
            types,
        }
    }
}

fn place(
    mut cursor: i64,
    entry: bool,
    body: &[Instruction],
    out: (&mut Option<(i64, u32)>, &mut HashMap<u32, i64>),
) -> i64 {
    for inst in body {
        if let Operation::Slot { size, align: req } = inst.op {
            cursor = align(cursor, i64::from(req.max(8)));
            if entry && out.0.is_none() {
                *out.0 = Some((cursor, size));
            }
            out.1.insert(inst.value, cursor);
            cursor += align(i64::from(size.max(8)), 8);
        }
    }
    cursor
}

pub fn suspends(op: &Operation) -> bool {
    matches!(
        op,
        Operation::Yield | Operation::Await { .. } | Operation::All { .. } | Operation::Race { .. }
    )
}

fn align(offset: i64, step: i64) -> i64 {
    let mask = step - 1;
    (offset + mask) & !mask
}

fn reserve(mut cursor: i64, id: u32, kind: &Type, values: &mut HashMap<u32, i64>) -> i64 {
    if !kind.concrete() {
        return cursor;
    }
    let step = i64::from(kind.alignment().max(8));
    cursor = align(cursor, step);
    values.insert(id, cursor);
    cursor + align(i64::from(kind.bytes().max(8)), 8)
}

struct Tracker {
    types: HashMap<u32, Type>,
    origin: HashMap<u32, usize>,
    cross: HashSet<u32>,
    ordered: Vec<u32>,
}

impl Tracker {
    fn parameter(&mut self, id: u32, kind: &Type, seg: usize) {
        self.types.insert(id, kind.clone());
        self.origin.insert(id, seg);
        if self.cross.insert(id) {
            self.ordered.push(id);
        }
    }

    fn define(&mut self, inst: &Instruction, slots: &HashMap<u32, i64>, seg: usize) {
        if !inst.kind.concrete() || slots.contains_key(&inst.value) {
            return;
        }
        self.types.insert(inst.value, inst.kind.clone());
        let home = match suspends(&inst.op) {
            true => 0,
            false => seg,
        };
        self.origin.insert(inst.value, home);
        if home == 0 && self.cross.insert(inst.value) {
            self.ordered.push(inst.value);
        }
    }

    fn check(&mut self, id: u32, seg: usize, slots: &HashMap<u32, i64>) {
        if slots.contains_key(&id) {
            return;
        }
        if let Some(&home) = self.origin.get(&id)
            && home != seg
            && self.cross.insert(id)
        {
            self.ordered.push(id);
        }
    }
}

fn liveness(function: &Function, slots: &HashMap<u32, i64>) -> (HashMap<u32, Type>, Vec<u32>) {
    let mut tracker = Tracker {
        types: HashMap::new(),
        origin: HashMap::new(),
        cross: HashSet::new(),
        ordered: Vec::new(),
    };
    for (id, kind) in &function.inputs {
        tracker.types.insert(*id, kind.clone());
    }
    let mut seg = 1usize;
    for block in &function.blocks {
        for (id, kind) in &block.parameters {
            tracker.parameter(*id, kind, seg);
        }
        for inst in &block.body {
            tracker.define(inst, slots, seg);
            if suspends(&inst.op) {
                seg += 1;
            }
        }
        seg += 1;
    }
    scan(function, slots, &mut tracker);
    (tracker.types, tracker.ordered)
}

fn scan(function: &Function, slots: &HashMap<u32, i64>, tracker: &mut Tracker) {
    let mut seg = 1usize;
    let mut refs = Vec::new();
    for block in &function.blocks {
        for inst in &block.body {
            refs.clear();
            operands(&inst.op, &mut refs);
            for &id in &refs {
                tracker.check(id, seg, slots);
            }
            if suspends(&inst.op) {
                seg += 1;
            }
        }
        refs.clear();
        exit(&block.exit, &mut refs);
        for &id in &refs {
            tracker.check(id, seg, slots);
        }
        seg += 1;
    }
}

fn operands(op: &Operation, out: &mut Vec<u32>) {
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
        other => more(other, out),
    }
}

fn more(op: &Operation, out: &mut Vec<u32>) {
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
            out.extend_from_slice(arguments)
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

fn exit(term: &Terminator, out: &mut Vec<u32>) {
    match term {
        Terminator::Jump(edge) => out.extend_from_slice(&edge.arguments),
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
            for (_, edge) in cases {
                out.extend_from_slice(&edge.arguments);
            }
            out.extend_from_slice(&default.arguments);
        }
        Terminator::Return(Some(val)) => out.push(*val),
        Terminator::Return(None) | Terminator::Trap { .. } => {}
    }
}
