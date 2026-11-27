use super::shape::Value;
use super::{Emitter, abi};
use crate::Fault;
use crate::intel::{Address, Condition, Label, Register, Width};
use crate::machine::Location;
use simplex_ir::{Block, Function, Target, Terminator, Type};

type Moves = Vec<(Location, Location)>;

impl Emitter<'_> {
    pub(crate) fn block(&mut self, func: &Function, block: &Block) -> Result<(), Fault> {
        self.encoder.bind(self.labels[&block.id]);
        for inst in &block.body {
            self.instruction(inst)?;
        }
        self.terminator(func, &block.exit)
    }

    fn terminator(&mut self, func: &Function, exit: &Terminator) -> Result<(), Fault> {
        match exit {
            Terminator::Jump(target) => self.edge(func, target),
            Terminator::Branch {
                condition,
                truthy,
                falsy,
            } => self.branch(func, *condition, (truthy, falsy)),
            Terminator::Switch {
                value,
                cases,
                default,
            } => self.switch(func, *value, cases, default),
            Terminator::Return(val) => {
                self.finish(*val, &func.output);
                Ok(())
            }
            Terminator::Trap { .. } => {
                self.encoder.trap();
                Ok(())
            }
        }
    }

    fn branch(
        &mut self,
        func: &Function,
        condition: u32,
        targets: (&Target, &Target),
    ) -> Result<(), Fault> {
        if let Value::General(c) = self.frame.value(condition) {
            self.read(Register::Rax, c);
            self.encoder.test(Register::Rax);
        }
        match targets.0.arguments.is_empty() {
            true => {
                self.encoder
                    .branch(Condition::Distinct, self.labels[&targets.0.block]);
                self.edge(func, targets.1)
            }
            false => {
                let bridge = self.encoder.fresh();
                self.encoder.branch(Condition::Distinct, bridge);
                self.edge(func, targets.1)?;
                self.encoder.bind(bridge);
                self.edge(func, targets.0)
            }
        }
    }

    fn switch(
        &mut self,
        func: &Function,
        value: u32,
        cases: &[(u64, Target)],
        default: &Target,
    ) -> Result<(), Fault> {
        let mut bridges = Vec::new();
        for (constant, target) in cases {
            if let Some(bridge) = self.arm(value, *constant, target) {
                bridges.push((bridge, target));
            }
        }
        self.edge(func, default)?;
        for (bridge, target) in bridges {
            self.encoder.bind(bridge);
            self.edge(func, target)?;
        }
        Ok(())
    }

    fn arm(&mut self, value: u32, constant: u64, target: &Target) -> Option<Label> {
        if let Value::General(v) = self.frame.value(value) {
            self.read(Register::Rax, v);
            self.encoder.constant(Register::Rcx, constant as i64);
            self.encoder.compare(Register::Rax, Register::Rcx);
        }
        match target.arguments.is_empty() {
            true => {
                self.encoder
                    .branch(Condition::Equal, self.labels[&target.block]);
                None
            }
            false => {
                let bridge = self.encoder.fresh();
                self.encoder.branch(Condition::Equal, bridge);
                Some(bridge)
            }
        }
    }

    fn edge(&mut self, func: &Function, target: &Target) -> Result<(), Fault> {
        let Some(&idx) = self.blocks.get(&target.block) else {
            return Err(Fault::Unsupported("missing target block".to_owned()));
        };
        let params = &func.blocks[idx].parameters;
        let mut gp = Moves::new();
        let mut fp = Moves::new();
        for (&(dst, _), &src) in params.iter().zip(&target.arguments) {
            if dst != src {
                self.assign((dst, src), &mut gp, &mut fp);
            }
        }
        self.shuffle(&mut gp);
        self.rearrange(&mut fp);
        self.encoder.jump(self.labels[&target.block]);
        Ok(())
    }

    fn assign(&mut self, edge: (u32, u32), gp: &mut Moves, fp: &mut Moves) {
        match (self.frame.value(edge.0), self.frame.value(edge.1)) {
            (Value::General(d), Value::General(s)) => {
                gp.push((self.frame.location(d), self.frame.location(s)));
            }
            (Value::Float { reg: d, .. }, Value::Float { reg: s, .. }) => {
                fp.push((self.frame.location(d), self.frame.location(s)));
            }
            (Value::Pair { low: dl, high: dh }, Value::Pair { low: sl, high: sh }) => {
                gp.push((self.frame.location(dl), self.frame.location(sl)));
                gp.push((self.frame.location(dh), self.frame.location(sh)));
            }
            (Value::Aggregate { slot: d, size }, Value::Aggregate { slot: s, .. }) if d != s => {
                self.encoder.address(
                    Register::R10,
                    Address::new(Register::Rbp, self.frame.base(d)),
                );
                self.encoder.address(
                    Register::R11,
                    Address::new(Register::Rbp, self.frame.base(s)),
                );
                self.encoder.replicate(Register::R10, Register::R11, size);
            }
            _ => {}
        }
    }

    fn finish(&mut self, val: Option<u32>, output: &Type) {
        let Some(id) = val else {
            self.epilogue();
            return;
        };
        if abi::indirect(self.frame.flavor, output) {
            if let Some(h) = self.frame.hidden {
                self.deliver(h, id);
            }
            self.epilogue();
            return;
        }
        match self.frame.value(id) {
            Value::Empty => {}
            Value::General(r) => {
                self.read(Register::Rax, r);
                self.normalize(Register::Rax, output);
            }
            Value::Float { reg, .. } => self.fetch(0, reg),
            Value::Pair { low, high } => {
                self.read(Register::R10, high);
                self.read(Register::Rax, low);
                self.encoder.copy(Register::Rdx, Register::R10);
            }
            Value::Aggregate { slot, size } if size <= 8 => {
                let src = Address::new(Register::Rbp, self.frame.base(slot));
                self.encoder.pack(Register::Rax, src, size);
            }
            Value::Aggregate { slot, size } => {
                let base = self.frame.base(slot);
                self.encoder.load(
                    Register::Rax,
                    Address::new(Register::Rbp, base),
                    Width::Quad,
                );
                self.encoder.pack(
                    Register::Rdx,
                    Address::new(Register::Rbp, base + 8),
                    size - 8,
                );
            }
        }
        self.epilogue();
    }

    fn deliver(&mut self, hidden: u32, id: u32) {
        match self.frame.value(id) {
            Value::Pair { low, high } => {
                self.read(Register::R10, hidden);
                self.read(Register::Rax, low);
                self.encoder
                    .store(Address::new(Register::R10, 0), Register::Rax, Width::Quad);
                self.read(Register::Rax, high);
                self.encoder
                    .store(Address::new(Register::R10, 8), Register::Rax, Width::Quad);
                self.encoder.copy(Register::Rax, Register::R10);
            }
            Value::Aggregate { slot, size } => {
                self.read(Register::R10, hidden);
                self.encoder.address(
                    Register::R11,
                    Address::new(Register::Rbp, self.frame.base(slot)),
                );
                self.encoder.replicate(Register::R10, Register::R11, size);
                self.read(Register::Rax, hidden);
            }
            _ => {}
        }
    }
}
