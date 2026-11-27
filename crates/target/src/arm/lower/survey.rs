use super::{Binding, Context, Local, Resolver};
use crate::Fault;
use crate::allocate::Plan;
use crate::arm::Encoder;
use crate::machine::Span;
use simplex_ir::{Function, Operation, Type};
use std::collections::BTreeMap;

pub struct Planner<'a> {
    pub function: &'a Function,
    pub kinds: Vec<Type>,
    pub bindings: Vec<Binding>,
    pub locals: Vec<Local>,
    pub explicit: BTreeMap<(usize, usize), usize>,
    pub copies: BTreeMap<(usize, usize, usize), usize>,
    pub indirect: Option<u32>,
    pub virtuals: u32,
    pub floats: Vec<bool>,
    pub ranges: Vec<Option<(u32, u32)>>,
    pub headers: BTreeMap<u32, u32>,
    pub edges: Vec<(u32, u32)>,
    pub calls: Vec<u32>,
    pub outgoing: u32,
}

impl<'a> Planner<'a> {
    pub fn new(function: &'a Function) -> Self {
        let mut limit = function.values as usize;
        for &(id, _) in &function.inputs {
            limit = limit.max(id as usize + 1);
        }
        for block in &function.blocks {
            for &(id, _) in &block.parameters {
                limit = limit.max(id as usize + 1);
            }
            for item in &block.body {
                limit = limit.max(item.value as usize + 1);
            }
        }
        Self {
            function,
            kinds: vec![Type::Void; limit],
            bindings: vec![Binding::Empty; limit],
            locals: Vec::new(),
            explicit: BTreeMap::new(),
            copies: BTreeMap::new(),
            indirect: None,
            virtuals: 0,
            floats: Vec::new(),
            ranges: Vec::new(),
            headers: BTreeMap::new(),
            edges: Vec::new(),
            calls: Vec::new(),
            outgoing: 0,
        }
    }

    pub fn alloc(&mut self, float: bool) -> u32 {
        let index = self.virtuals;
        self.virtuals += 1;
        self.floats.push(float);
        self.ranges.push(None);
        index
    }

    pub fn touch(&mut self, value: u32, point: u32) {
        let slot = &mut self.ranges[value as usize];
        *slot = Some(slot.map_or((point, point), |(s, e)| (s.min(point), e.max(point))));
    }

    pub fn mark(&mut self, binding: Binding, point: u32) {
        match binding {
            Binding::Single(v) => self.touch(v, point),
            Binding::Pair(low, high) => {
                self.touch(low, point);
                self.touch(high, point);
            }
            Binding::Empty | Binding::Slot(_) => {}
        }
    }

    fn bind(&mut self, value: u32, kind: &Type) {
        self.kinds[value as usize] = kind.clone();
        self.bindings[value as usize] = match kind {
            Type::Void => Binding::Empty,
            Type::Float { .. } => Binding::Single(self.alloc(true)),
            Type::Pair => Binding::Pair(self.alloc(false), self.alloc(false)),
            Type::Aggregate { size, align, .. } => {
                let slot = self.locals.len();
                self.locals.push(Local {
                    size: (*size).max(8),
                    align: (*align).clamp(8, 16),
                    offset: 0,
                });
                Binding::Slot(slot)
            }
            _ => Binding::Single(self.alloc(false)),
        };
    }

    fn reserve(&mut self, site: (usize, usize), op: &Operation) -> Result<(), Fault> {
        match op {
            Operation::Slot { size, align } => {
                let index = self.locals.len();
                self.locals.push(Local {
                    size: (*size).max(8),
                    align: (*align).clamp(8, 16),
                    offset: 0,
                });
                self.explicit.insert(site, index);
            }
            Operation::Call { inputs, .. } | Operation::Indirect { inputs, .. } => {
                self.stage(site, inputs);
            }
            Operation::Spawn { .. }
            | Operation::Yield
            | Operation::Await { .. }
            | Operation::All { .. }
            | Operation::Race { .. } => {
                return Err(Fault::Unsupported(String::from(
                    "unlowered fiber operation",
                )));
            }
            _ => {}
        }
        Ok(())
    }

    fn stage(&mut self, site: (usize, usize), inputs: &[Type]) {
        for (arg, kind) in inputs.iter().enumerate() {
            if let Type::Aggregate { size, align, .. } = *kind
                && size > 16
            {
                let index = self.locals.len();
                self.locals.push(Local {
                    size,
                    align: align.clamp(8, 16),
                    offset: 0,
                });
                self.copies.insert((site.0, site.1, arg), index);
            }
        }
    }

    pub fn populate(&mut self) -> Result<(), Fault> {
        if let Type::Aggregate { size, .. } = self.function.output
            && size > 16
        {
            self.indirect = Some(self.alloc(false));
        }
        for (id, kind) in &self.function.inputs {
            self.bind(*id, kind);
        }
        for (bidx, block) in self.function.blocks.iter().enumerate() {
            for (id, kind) in &block.parameters {
                self.bind(*id, kind);
            }
            for (iidx, item) in block.body.iter().enumerate() {
                self.bind(item.value, &item.kind);
                self.reserve((bidx, iidx), &item.op)?;
            }
        }
        self.scan();
        Ok(())
    }

    pub fn spans(&mut self) -> Vec<Span> {
        self.widen();
        self.ranges
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let (start, end) = (*item)?;
                let across = self.calls.iter().any(|&c| start < c && end > c);
                Some(Span {
                    value: idx as u32,
                    start,
                    end,
                    float: self.floats[idx],
                    across,
                })
            })
            .collect()
    }

    pub fn assemble(
        mut self,
        resolver: &'a dyn Resolver,
        plan: Plan,
        collector: fn(&Plan) -> (Vec<u8>, Vec<u8>),
    ) -> Context<'a> {
        let mut cursor = (self.outgoing + 15) & !15;
        for slot in &mut self.locals {
            let mask = slot.align - 1;
            cursor = (cursor + mask) & !mask;
            slot.offset = cursor;
            cursor += (slot.size + 7) & !7;
        }
        cursor = (cursor + 15) & !15;
        let spills = cursor;
        cursor += (plan.frame + 15) & !15;
        let (saved, media) = collector(&plan);
        let goff = cursor;
        cursor += (((saved.len() + 1) & !1) * 8) as u32;
        let foff = cursor;
        cursor += (((media.len() + 1) & !1) * 8) as u32;
        let mut encoder = Encoder::new();
        let mut labels = BTreeMap::new();
        for block in &self.function.blocks {
            labels.insert(block.id, encoder.fresh());
        }
        Context {
            function: self.function,
            resolver,
            encoder,
            kinds: self.kinds,
            bindings: self.bindings,
            locals: self.locals,
            explicit: self.explicit,
            copies: self.copies,
            labels,
            plan,
            indirect: self.indirect,
            spills,
            general: (goff, saved),
            float: (foff, media),
            total: cursor,
        }
    }
}
