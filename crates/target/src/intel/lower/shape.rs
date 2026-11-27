use super::{abi, flow, portable};
use crate::Fault;
use crate::allocate::{Plan, assign};
use crate::intel::{Flavor, Precision, Register};
use crate::machine::Location;
use simplex_ir::{Block, Function, Instruction, Operation, Type};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Empty,
    General(u32),
    Float { reg: u32, prec: Precision },
    Pair { low: u32, high: u32 },
    Aggregate { slot: usize, size: u32 },
}

pub(crate) struct Frame {
    pub(crate) flavor: Flavor,
    pub(crate) values: Vec<Value>,
    pub(crate) hidden: Option<u32>,
    pub(crate) allocations: BTreeMap<u32, usize>,
    pub(crate) locations: BTreeMap<u32, Location>,
    pub(crate) used: Vec<u8>,
    pub(crate) bases: Vec<i32>,
    pub(crate) spills: i32,
    pub(crate) reserve: i32,
    pub(crate) order: Vec<usize>,
}

impl Frame {
    pub(crate) fn value(&self, idx: u32) -> Value {
        self.values
            .get(idx as usize)
            .copied()
            .unwrap_or(Value::Empty)
    }

    pub(crate) fn location(&self, reg: u32) -> Location {
        self.locations
            .get(&reg)
            .copied()
            .unwrap_or(Location::General(Register::R10.code()))
    }

    pub(crate) fn base(&self, slot: usize) -> i32 {
        self.bases[slot]
    }

    pub(crate) fn spill(&self, off: i32) -> i32 {
        -(self.spills + off + 8)
    }

    pub(crate) fn allocation(&self, inst: u32) -> usize {
        self.allocations.get(&inst).copied().unwrap_or(0)
    }
}

pub(crate) struct Planner {
    pub(crate) flavor: Flavor,
    pub(crate) values: Vec<Value>,
    pub(crate) floats: Vec<bool>,
    pub(crate) slots: Vec<(u32, u32)>,
    pub(crate) allocations: BTreeMap<u32, usize>,
    pub(crate) outgoing: u32,
}

impl Planner {
    fn new(flavor: Flavor, limit: usize) -> Self {
        Self {
            flavor,
            values: vec![Value::Empty; limit],
            floats: Vec::new(),
            slots: Vec::new(),
            allocations: BTreeMap::new(),
            outgoing: 0,
        }
    }

    fn general(&mut self) -> u32 {
        let id = self.floats.len() as u32;
        self.floats.push(false);
        id
    }

    fn float(&mut self) -> u32 {
        let id = self.floats.len() as u32;
        self.floats.push(true);
        id
    }

    fn slot(&mut self, bytes: u32, align: u32) -> usize {
        let idx = self.slots.len();
        self.slots.push((bytes.max(8), align.max(8)));
        idx
    }

    fn bind(&mut self, id: u32, kind: &Type) {
        let val = match kind {
            Type::Void | Type::Aggregate { size: 0, .. } => Value::Empty,
            Type::Integer { .. } | Type::Pointer => Value::General(self.general()),
            Type::Float { bits } => Value::Float {
                reg: self.float(),
                prec: Precision::bits(*bits),
            },
            Type::Pair => Value::Pair {
                low: self.general(),
                high: self.general(),
            },
            Type::Aggregate { size, align, .. } => Value::Aggregate {
                slot: self.slot(*size, *align),
                size: *size,
            },
        };
        self.values[id as usize] = val;
    }

    pub(crate) fn regs(&self, id: u32, out: &mut Vec<u32>) {
        match self
            .values
            .get(id as usize)
            .copied()
            .unwrap_or(Value::Empty)
        {
            Value::General(r) | Value::Float { reg: r, .. } => out.push(r),
            Value::Pair { low, high } => {
                out.push(low);
                out.push(high);
            }
            Value::Empty | Value::Aggregate { .. } => {}
        }
    }

    fn scan(&mut self, func: &Function) -> Result<(), Fault> {
        for block in &func.blocks {
            for (id, kind) in &block.parameters {
                self.bind(*id, kind);
            }
            for inst in &block.body {
                self.bind(inst.value, &inst.kind);
                self.inspect(inst)?;
            }
        }
        Ok(())
    }

    fn inspect(&mut self, inst: &Instruction) -> Result<(), Fault> {
        match &inst.op {
            Operation::Slot { size, align } => {
                let idx = self.slot(*size, *align);
                self.allocations.insert(inst.value, idx);
            }
            Operation::Call { inputs, kind, .. } | Operation::Indirect { inputs, kind, .. } => {
                let space = portable::space(self.flavor, inputs, kind);
                self.outgoing = self.outgoing.max(space);
            }
            Operation::Spawn { .. }
            | Operation::Yield
            | Operation::Await { .. }
            | Operation::All { .. }
            | Operation::Race { .. } => {
                return Err(Fault::Unsupported("unprepared fiber operation".to_owned()));
            }
            _ => {}
        }
        Ok(())
    }

    fn assemble(self, hidden: Option<u32>, order: Vec<usize>, plan: Plan) -> Frame {
        let saved = (plan.used.len() * 8) as i32;
        let mut cursor = saved;
        let mut bases = Vec::with_capacity(self.slots.len());
        for (bytes, align) in self.slots {
            let a = align as i32;
            let b = ((bytes + 7) & !7) as i32;
            cursor = (cursor + b + a - 1) & !(a - 1);
            bases.push(-cursor);
        }
        let spills = cursor;
        cursor += plan.frame as i32;
        let total = (cursor + self.outgoing as i32 + 15) & !15;
        Frame {
            flavor: self.flavor,
            values: self.values,
            hidden,
            allocations: self.allocations,
            locations: plan.locations,
            used: plan.used,
            bases,
            spills,
            reserve: total - saved,
            order,
        }
    }
}

pub(crate) fn build(flavor: Flavor, func: &Function) -> Result<Frame, Fault> {
    if func.asynchronous {
        return Err(Fault::Unsupported("unprepared async function".to_owned()));
    }
    if func.blocks.is_empty() {
        return Err(Fault::Unsupported("empty function".to_owned()));
    }
    let mut planner = Planner::new(flavor, ceiling(func));
    let hidden = abi::indirect(flavor, &func.output).then(|| planner.general());
    for (id, kind) in &func.inputs {
        planner.bind(*id, kind);
    }
    planner.scan(func)?;
    let ordered = order(&func.blocks);
    let spans = flow::liveness(func, &planner, hidden, &ordered);
    let plan = assign(&Register::bank(flavor), &spans);
    Ok(planner.assemble(hidden, ordered, plan))
}

fn order(blocks: &[Block]) -> Vec<usize> {
    let map: BTreeMap<u32, usize> = blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut seen = vec![false; blocks.len()];
    let mut post = Vec::with_capacity(blocks.len());
    visit(0, blocks, &map, (&mut seen, &mut post));
    post.reverse();
    post
}

fn visit(
    idx: usize,
    blocks: &[Block],
    map: &BTreeMap<u32, usize>,
    state: (&mut [bool], &mut Vec<usize>),
) {
    if idx >= blocks.len() || state.0[idx] {
        return;
    }
    state.0[idx] = true;
    for succ in flow::successors(&blocks[idx].exit) {
        if let Some(&next) = map.get(&succ.block) {
            visit(next, blocks, map, (state.0, state.1));
        }
    }
    state.1.push(idx);
}

fn ceiling(func: &Function) -> usize {
    let mut top = func.values;
    for &(id, _) in &func.inputs {
        top = top.max(id + 1);
    }
    for block in &func.blocks {
        top = block
            .parameters
            .iter()
            .fold(top, |acc, &(id, _)| acc.max(id + 1));
        top = block
            .body
            .iter()
            .fold(top, |acc, inst| acc.max(inst.value + 1));
    }
    top.max(1) as usize
}
