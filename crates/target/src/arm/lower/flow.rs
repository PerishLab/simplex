use super::shuffle::Moves;
use super::{Binding, Context};
use crate::Fault;
use crate::arm::{Condition, Precision};
use crate::machine::Location;
use simplex_ir::{Target, Terminator, Type};

impl Context<'_> {
    pub fn terminator(&mut self, exit: &Terminator) -> Result<(), Fault> {
        match exit {
            Terminator::Jump(target) => self.route(target)?,
            Terminator::Branch {
                condition,
                truthy,
                falsy,
            } => self.fork(*condition, truthy, falsy)?,
            Terminator::Switch {
                value,
                cases,
                default,
            } => self.select(*value, cases, default)?,
            Terminator::Return(value) => {
                self.deliver(*value);
                self.epilogue();
            }
            Terminator::Trap { .. } => self.encoder.trap(),
        }
        Ok(())
    }

    fn fork(&mut self, condition: u32, truthy: &Target, falsy: &Target) -> Result<(), Fault> {
        let reg = match self.bindings[condition as usize] {
            Binding::Single(vid) => self.get(vid, 15),
            _ => 15,
        };
        let yes = self.encoder.fresh();
        self.encoder.zero(reg, true, yes);
        self.route(falsy)?;
        self.encoder.bind(yes);
        self.route(truthy)
    }

    fn select(
        &mut self,
        value: u32,
        cases: &[(u64, Target)],
        default: &Target,
    ) -> Result<(), Fault> {
        let domain = self.kinds[value as usize].clone();
        for (constant, target) in cases {
            let next = self.encoder.fresh();
            let reg = match self.bindings[value as usize] {
                Binding::Single(vid) => self.get(vid, 14),
                _ => 14,
            };
            self.normalize(14, reg, &domain);
            self.encoder.constant(15, *constant);
            self.normalize(15, 15, &domain);
            self.encoder.compare(14, 15);
            self.encoder.branch(Condition::Distinct, next);
            self.route(target)?;
            self.encoder.bind(next);
        }
        self.route(default)
    }

    fn deliver(&mut self, value: Option<u32>) {
        let Some(id) = value else {
            return;
        };
        let output = self.function.output.clone();
        match (&output, self.bindings[id as usize]) {
            (Type::Void, _) | (_, Binding::Empty) => {}
            (Type::Float { bits }, Binding::Single(vid)) => {
                let precision = Precision::from(*bits);
                let freg = self.fetch(vid, 16, precision);
                self.encoder.duplicate(0, freg, precision);
            }
            (Type::Pair, Binding::Pair(low, high)) => {
                let lv = self.get(low, 14);
                let hv = self.get(high, 15);
                self.encoder.copy(0, lv);
                self.encoder.copy(1, hv);
            }
            (Type::Aggregate { size, .. }, Binding::Slot(slot)) => {
                self.aggregate(*size, slot);
            }
            (_, Binding::Single(vid)) => {
                let reg = self.get(vid, 15);
                self.normalize(0, reg, &output);
            }
            _ => {}
        }
    }

    fn aggregate(&mut self, size: u32, slot: usize) {
        let offset = i64::from(self.locals[slot].offset);
        if size > 0 && size <= 8 {
            self.partial(0, (31, offset), size);
            return;
        }
        if size > 8 && size <= 16 {
            self.partial(0, (31, offset), 8);
            self.partial(1, (31, offset + 8), size - 8);
            return;
        }
        if size > 16
            && let Some(vid) = self.indirect
        {
            let ptr = self.get(vid, 14);
            self.encoder.copy(14, ptr);
            self.encoder.displace(17, 31, offset);
            self.encoder.transfer(14, 17, size);
        }
    }

    fn route(&mut self, target: &Target) -> Result<(), Fault> {
        let Some(&label) = self.labels.get(&target.block) else {
            return Err(Fault::Unsupported(String::from("missing target block")));
        };
        if !target.arguments.is_empty()
            && let Some(block) = self.function.blocks.iter().find(|b| b.id == target.block)
        {
            let params = block.parameters.clone();
            let mut moves = Moves::default();
            for (&arg, &(param, ref kind)) in target.arguments.iter().zip(params.iter()) {
                self.record((param, arg), kind, &mut moves);
            }
            self.blocks(&mut moves.blocks);
            self.shuffle(&mut moves.general);
            self.permute(&mut moves.media);
        }
        self.encoder.jump(label);
        Ok(())
    }

    fn record(&self, ids: (u32, u32), kind: &Type, moves: &mut Moves) {
        let (param, arg) = ids;
        match (
            kind,
            self.bindings[param as usize],
            self.bindings[arg as usize],
        ) {
            (Type::Float { bits }, Binding::Single(d), Binding::Single(s)) => {
                if let (Some(dl), Some(sl)) = (
                    self.plan.locations.get(&d).copied(),
                    self.plan.locations.get(&s).copied(),
                ) {
                    moves.media.push((dl, sl, Precision::from(*bits)));
                }
            }
            (Type::Pair, Binding::Pair(dl, dh), Binding::Pair(sl, sh)) => {
                self.append(dl, sl, &mut moves.general);
                self.append(dh, sh, &mut moves.general);
            }
            (Type::Aggregate { size, .. }, Binding::Slot(d), Binding::Slot(s)) => {
                if d != s && *size > 0 {
                    moves.blocks.push((d, s, *size));
                }
            }
            (_, Binding::Single(d), Binding::Single(s)) => {
                self.append(d, s, &mut moves.general);
            }
            _ => {}
        }
    }

    fn append(&self, dst: u32, src: u32, list: &mut Vec<(Location, Location)>) {
        if let (Some(dl), Some(sl)) = (
            self.plan.locations.get(&dst).copied(),
            self.plan.locations.get(&src).copied(),
        ) {
            list.push((dl, sl));
        }
    }
}
