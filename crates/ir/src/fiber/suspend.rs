use super::batch::{CONTEST, PACK, ROUND};
use super::builder::{SIGNED, UNSIGNED};
use super::layout::{CHILD, RESULT, STATE};
use super::runtime::{DROP, SWEEP, TICK};
use super::step::Worker;
use crate::{Binary, Instruction, Operation, Terminator, Type};
use std::collections::HashMap;

impl Worker<'_> {
    pub(super) fn suspend(&mut self, inst: &Instruction, locals: &mut HashMap<u32, u32>) -> u32 {
        let tag = (self.states.len() as u64) + 1;
        let next = match &inst.op {
            Operation::Yield => self.pause(tag),
            Operation::Await { fiber, kind } => self.wait(tag, (*fiber, inst.value, kind), locals),
            Operation::All {
                fibers,
                count,
                element,
            } => self.gather(tag, (*fibers, *count, inst.value), element, locals),
            Operation::Race {
                fibers,
                count,
                kind,
            } => self.race(tag, (*fibers, *count, inst.value), kind, locals),
            _ => self.b.block(),
        };
        locals.clear();
        next
    }

    fn pause(&mut self, tag: u64) -> u32 {
        let resume = self.b.block();
        self.states.push((tag, resume));
        let nil = self.b.null();
        let cptr = self.b.offset(self.frame, CHILD);
        self.b.store(cptr, nil, Type::Pointer);
        let st = self.b.unsigned(tag);
        let sptr = self.b.offset(self.frame, STATE);
        self.b.store(sptr, st, UNSIGNED);
        let zero = self.b.signed(0);
        let cur = self.b.active;
        self.b.blocks[cur].exit = Terminator::Return(Some(zero));
        resume
    }

    fn wait(&mut self, tag: u64, spec: (u32, u32, &Type), locals: &mut HashMap<u32, u32>) -> u32 {
        let handle = self.fetch(spec.0, locals);
        let poll = self.b.block();
        let pending = self.b.block();
        let ready = self.b.block();
        let resume = self.b.block();
        self.states.push((tag, poll));
        let cptr = self.b.offset(self.frame, CHILD);
        self.b.store(cptr, handle, Type::Pointer);
        let st = self.b.unsigned(tag);
        let sptr = self.b.offset(self.frame, STATE);
        self.b.store(sptr, st, UNSIGNED);
        self.b.jump(poll, Vec::new());
        self.b.select(poll);
        let cp = self.b.offset(self.frame, CHILD);
        let sub = self.b.load(cp, Type::Pointer);
        let res = self.b.call(TICK, vec![sub], vec![Type::Pointer], SIGNED);
        let zero = self.b.signed(0);
        let done = self.b.binary(Binary::Ne, res, zero, SIGNED);
        self.b
            .branch(done, (ready, Vec::new()), (pending, Vec::new()));
        self.b.select(pending);
        let ret0 = self.b.signed(0);
        self.b.blocks[pending as usize].exit = Terminator::Return(Some(ret0));
        self.b.select(ready);
        let nil = self.b.null();
        let cp2 = self.b.offset(self.frame, CHILD);
        self.b.store(cp2, nil, Type::Pointer);
        if spec.2.concrete() {
            let rptr = self.b.offset(sub, RESULT);
            let val = self.b.load(rptr, spec.2.clone());
            self.spill(spec.1, val, spec.2.clone());
        }
        let _ = self
            .b
            .call(DROP, vec![sub], vec![Type::Pointer], Type::Void);
        self.b.jump(resume, Vec::new());
        resume
    }

    fn gather(
        &mut self,
        tag: u64,
        batch: (u32, u32, u32),
        element: &Type,
        locals: &mut HashMap<u32, u32>,
    ) -> u32 {
        let arr = self.fetch(batch.0, locals);
        let len = self.unsigned(batch.1, locals);
        let poll = self.b.block();
        let pending = self.b.block();
        let ready = self.b.block();
        let resume = self.b.block();
        self.states.push((tag, poll));
        let bptr = self.b.offset(self.frame, self.plan.batch);
        let cptr = self.b.offset(self.frame, self.plan.count);
        self.b.store(bptr, arr, Type::Pointer);
        self.b.store(cptr, len, UNSIGNED);
        let st = self.b.unsigned(tag);
        let sptr = self.b.offset(self.frame, STATE);
        self.b.store(sptr, st, UNSIGNED);
        self.b.jump(poll, Vec::new());
        self.b.select(poll);
        let bp = self.b.offset(self.frame, self.plan.batch);
        let cp = self.b.offset(self.frame, self.plan.count);
        let curarr = self.b.load(bp, Type::Pointer);
        let curlen = self.b.load(cp, UNSIGNED);
        let res = self.b.call(
            ROUND,
            vec![curarr, curlen],
            vec![Type::Pointer, UNSIGNED],
            SIGNED,
        );
        let zero = self.b.signed(0);
        let done = self.b.binary(Binary::Ne, res, zero, SIGNED);
        self.b
            .branch(done, (ready, Vec::new()), (pending, Vec::new()));
        self.b.select(pending);
        let ret0 = self.b.signed(0);
        self.b.blocks[pending as usize].exit = Terminator::Return(Some(ret0));
        self.b.select(ready);
        let nil = self.b.null();
        let z0 = self.b.unsigned(0);
        let bp2 = self.b.offset(self.frame, self.plan.batch);
        let cp2 = self.b.offset(self.frame, self.plan.count);
        self.b.store(bp2, nil, Type::Pointer);
        self.b.store(cp2, z0, UNSIGNED);
        let bytes = self.b.unsigned(u64::from(element.bytes()));
        let pair = self.b.call(
            PACK,
            vec![curarr, curlen, bytes],
            vec![Type::Pointer, UNSIGNED, UNSIGNED],
            Type::Pair,
        );
        self.spill(batch.2, pair, Type::Pair);
        self.b.jump(resume, Vec::new());
        resume
    }

    fn race(
        &mut self,
        tag: u64,
        batch: (u32, u32, u32),
        kind: &Type,
        locals: &mut HashMap<u32, u32>,
    ) -> u32 {
        let arr = self.fetch(batch.0, locals);
        let len = self.unsigned(batch.1, locals);
        let poll = self.b.block();
        let pending = self.b.block();
        let settle = self.b.block();
        let copy = self.b.block();
        let sweep = self.b.block();
        let resume = self.b.block();
        self.states.push((tag, poll));
        let bptr = self.b.offset(self.frame, self.plan.batch);
        let cptr = self.b.offset(self.frame, self.plan.count);
        self.b.store(bptr, arr, Type::Pointer);
        self.b.store(cptr, len, UNSIGNED);
        let st = self.b.unsigned(tag);
        let sptr = self.b.offset(self.frame, STATE);
        self.b.store(sptr, st, UNSIGNED);
        self.b.jump(poll, Vec::new());
        self.b.select(poll);
        let bp = self.b.offset(self.frame, self.plan.batch);
        let cp = self.b.offset(self.frame, self.plan.count);
        let curarr = self.b.load(bp, Type::Pointer);
        let curlen = self.b.load(cp, UNSIGNED);
        let win = self.b.call(
            CONTEST,
            vec![curarr, curlen],
            vec![Type::Pointer, UNSIGNED],
            SIGNED,
        );
        let m1 = self.b.signed(-1);
        let wait = self.b.binary(Binary::Eq, win, m1, SIGNED);
        self.b
            .branch(wait, (pending, Vec::new()), (settle, Vec::new()));
        self.b.select(pending);
        let ret0 = self.b.signed(0);
        self.b.blocks[pending as usize].exit = Terminator::Return(Some(ret0));
        self.b.select(settle);
        let nil = self.b.null();
        let z0 = self.b.unsigned(0);
        let bp2 = self.b.offset(self.frame, self.plan.batch);
        let cp2 = self.b.offset(self.frame, self.plan.count);
        self.b.store(bp2, nil, Type::Pointer);
        self.b.store(cp2, z0, UNSIGNED);
        let zero = self.b.signed(0);
        let valid = self.b.binary(Binary::Ge, win, zero, SIGNED);
        self.b
            .branch(valid, (copy, Vec::new()), (sweep, Vec::new()));
        self.b.select(copy);
        if kind.concrete() {
            let idx = self.b.bitcast(win, SIGNED, UNSIGNED);
            let eight = self.b.unsigned(8);
            let off = self.b.binary(Binary::Mul, idx, eight, UNSIGNED);
            let slot = self.b.advance(curarr, off);
            let item = self.b.load(slot, Type::Pointer);
            let rptr = self.b.offset(item, RESULT);
            let val = self.b.load(rptr, kind.clone());
            self.spill(batch.2, val, kind.clone());
        }
        self.b.jump(sweep, Vec::new());
        self.b.select(sweep);
        let _ = self.b.call(
            SWEEP,
            vec![curarr, curlen],
            vec![Type::Pointer, UNSIGNED],
            Type::Void,
        );
        self.b.jump(resume, Vec::new());
        resume
    }
}
