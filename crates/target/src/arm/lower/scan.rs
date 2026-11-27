use super::footprint;
use super::survey::Planner;
use simplex_ir::{Operation, Target, Terminator, Type};

impl Planner<'_> {
    pub fn scan(&mut self) {
        if let Some(vid) = self.indirect {
            self.touch(vid, 0);
        }
        for &(id, _) in &self.function.inputs {
            self.mark(self.bindings[id as usize], 0);
        }
        let mut point = 2u32;
        for block in &self.function.blocks {
            for &(id, _) in &block.parameters {
                self.mark(self.bindings[id as usize], point - 1);
            }
            self.headers.insert(block.id, point);
            point += 1;
            for item in &block.body {
                self.operands(&item.op, point);
                self.mark(self.bindings[item.value as usize], point);
                point += 1;
            }
            self.exits(&block.exit, point);
            point += 2;
        }
    }

    pub fn widen(&mut self) {
        let mut changed = true;
        while changed {
            changed = false;
            for index in 0..self.edges.len() {
                let (latch, target) = self.edges[index];
                if let Some(&header) = self.headers.get(&target)
                    && header <= latch
                {
                    changed |= self.stretch(header, latch);
                }
            }
        }
    }

    fn stretch(&mut self, header: u32, latch: u32) -> bool {
        let mut updated = false;
        for range in self.ranges.iter_mut().flatten() {
            if range.0 <= header && range.1 >= header && range.1 < latch {
                range.1 = latch;
                updated = true;
            }
        }
        updated
    }

    fn operands(&mut self, op: &Operation, point: u32) {
        match op {
            Operation::Unary { value, .. }
            | Operation::Cast { value, .. }
            | Operation::Low { pair: value, .. }
            | Operation::High { pair: value, .. }
            | Operation::Offset { base: value, .. }
            | Operation::Load { address: value, .. }
            | Operation::Acquire { address: value, .. } => {
                self.mark(self.bindings[*value as usize], point);
            }
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
                self.mark(self.bindings[*left as usize], point);
                self.mark(self.bindings[*right as usize], point);
            }
            Operation::Swap {
                address,
                expected,
                desired,
                ..
            } => {
                self.mark(self.bindings[*address as usize], point);
                self.mark(self.bindings[*expected as usize], point);
                self.mark(self.bindings[*desired as usize], point);
            }
            Operation::Call {
                arguments, inputs, ..
            } => self.observe(None, arguments, inputs, point),
            Operation::Indirect {
                callee,
                arguments,
                inputs,
                ..
            } => self.observe(Some(*callee), arguments, inputs, point),
            _ => {}
        }
    }

    fn observe(&mut self, callee: Option<u32>, arguments: &[u32], inputs: &[Type], point: u32) {
        self.calls.push(point);
        self.outgoing = self.outgoing.max(footprint(inputs));
        if let Some(target) = callee {
            self.mark(self.bindings[target as usize], point);
        }
        for &arg in arguments {
            self.mark(self.bindings[arg as usize], point);
        }
    }

    fn exits(&mut self, exit: &Terminator, point: u32) {
        match exit {
            Terminator::Jump(target) => self.edge(target, point),
            Terminator::Branch {
                condition,
                truthy,
                falsy,
            } => {
                self.mark(self.bindings[*condition as usize], point);
                self.edge(truthy, point);
                self.edge(falsy, point);
            }
            Terminator::Switch {
                value,
                cases,
                default,
            } => {
                self.mark(self.bindings[*value as usize], point);
                for (_, target) in cases {
                    self.edge(target, point);
                }
                self.edge(default, point);
            }
            Terminator::Return(value) => {
                if let Some(vid) = self.indirect {
                    self.touch(vid, point);
                }
                if let Some(id) = value {
                    self.mark(self.bindings[*id as usize], point);
                }
            }
            Terminator::Trap { .. } => {}
        }
    }

    fn edge(&mut self, target: &Target, point: u32) {
        for &arg in &target.arguments {
            self.mark(self.bindings[arg as usize], point);
        }
        self.edges.push((point, target.block));
    }
}
