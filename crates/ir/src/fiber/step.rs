use super::builder::{Builder, SIGNED, UNSIGNED};
use super::layout::{CANCELLED, CHILD, COMPLETED, Plan, RESULT, STATE, suspends};
use crate::{Binary, Function, Instruction, Linkage, Operation, Target, Terminator, Type};
use std::collections::HashMap;

pub fn compile(function: &Function, plan: &Plan, name: &str) -> Function {
    let mut b = Builder::new(name.to_owned(), Linkage::Internal, SIGNED);
    let frame = b.input(Type::Pointer);
    let dispatch = b.block();
    let settled = b.block();
    let switchblk = b.block();
    let mut map = HashMap::with_capacity(function.blocks.len());
    for orig in &function.blocks {
        let dst = b.block();
        map.insert(orig.id, dst);
        for (_, kind) in &orig.parameters {
            let _ = b.parameter(dst, kind.clone());
        }
    }
    let mut worker = Worker {
        b: &mut b,
        function,
        plan,
        frame,
        map,
        states: Vec::new(),
    };
    for orig in &function.blocks {
        worker.block(orig);
    }
    let states = worker.states;
    let start = worker.map[&function.blocks[0].id];
    b.select(dispatch);
    let stptr = b.offset(frame, STATE);
    let state = b.load(stptr, UNSIGNED);
    let limit = b.unsigned(CANCELLED);
    let done = b.binary(Binary::Ge, state, limit, UNSIGNED);
    b.branch(done, (settled, Vec::new()), (switchblk, Vec::new()));
    b.select(settled);
    let one = b.signed(1);
    b.blocks[settled as usize].exit = Terminator::Return(Some(one));
    b.select(switchblk);
    if states.is_empty() {
        b.jump(start, Vec::new());
    } else {
        let cases = states
            .into_iter()
            .map(|(tag, blk)| {
                (
                    tag,
                    Target {
                        block: blk,
                        arguments: Vec::new(),
                    },
                )
            })
            .collect();
        b.blocks[switchblk as usize].exit = Terminator::Switch {
            value: state,
            cases,
            default: Target {
                block: start,
                arguments: Vec::new(),
            },
        };
    }
    b.finish()
}

pub(super) struct Worker<'a> {
    pub(super) b: &'a mut Builder,
    pub(super) function: &'a Function,
    pub(super) plan: &'a Plan,
    pub(super) frame: u32,
    pub(super) map: HashMap<u32, u32>,
    pub(super) states: Vec<(u64, u32)>,
}

impl Worker<'_> {
    fn block(&mut self, orig: &crate::Block) {
        let mut current = self.map[&orig.id];
        self.b.select(current);
        let mut locals: HashMap<u32, u32> = HashMap::new();
        let params = self.b.blocks[current as usize].parameters.clone();
        for ((oid, _), (lid, kind)) in orig.parameters.iter().zip(params) {
            locals.insert(*oid, lid);
            self.spill(*oid, lid, kind);
        }
        for inst in &orig.body {
            if suspends(&inst.op) {
                current = self.suspend(inst, &mut locals);
                self.b.select(current);
            } else {
                self.instruction(inst, &mut locals);
            }
        }
        self.ending(&orig.exit, &mut locals);
    }

    pub(super) fn spill(&mut self, orig: u32, local: u32, kind: Type) {
        if let Some(&off) = self.plan.values.get(&orig) {
            let addr = self.b.offset(self.frame, off);
            self.b.store(addr, local, kind);
        }
    }

    pub(super) fn fetch(&mut self, orig: u32, locals: &mut HashMap<u32, u32>) -> u32 {
        if let Some(&existing) = locals.get(&orig) {
            return existing;
        }
        if let Some(&off) = self.plan.slots.get(&orig) {
            let ptr = self.b.offset(self.frame, off);
            locals.insert(orig, ptr);
            return ptr;
        }
        let off = self.plan.values[&orig];
        let kind = self.plan.types[&orig].clone();
        let addr = self.b.offset(self.frame, off);
        let val = self.b.load(addr, kind);
        locals.insert(orig, val);
        val
    }

    fn instruction(&mut self, inst: &Instruction, locals: &mut HashMap<u32, u32>) {
        if let Operation::Slot { .. } = inst.op {
            let off = self.plan.slots[&inst.value];
            let ptr = self.b.offset(self.frame, off);
            locals.insert(inst.value, ptr);
            return;
        }
        let op = self.remap(&inst.op, locals);
        let val = self.b.emit(inst.kind.clone(), op);
        if inst.kind.concrete() {
            locals.insert(inst.value, val);
            self.spill(inst.value, val, inst.kind.clone());
        }
    }

    pub(super) fn unsigned(&mut self, id: u32, locals: &mut HashMap<u32, u32>) -> u32 {
        let val = self.fetch(id, locals);
        match self.plan.types.get(&id) {
            Some(Type::Integer {
                bits: 64,
                signed: true,
            }) => self.b.bitcast(val, SIGNED, UNSIGNED),
            _ => val,
        }
    }

    fn ending(&mut self, exit: &Terminator, locals: &mut HashMap<u32, u32>) {
        let translated = match exit {
            Terminator::Jump(item) => Terminator::Jump(self.edge(item, locals)),
            Terminator::Branch {
                condition,
                truthy,
                falsy,
            } => Terminator::Branch {
                condition: self.fetch(*condition, locals),
                truthy: self.edge(truthy, locals),
                falsy: self.edge(falsy, locals),
            },
            Terminator::Switch {
                value,
                cases,
                default,
            } => Terminator::Switch {
                value: self.fetch(*value, locals),
                cases: cases
                    .iter()
                    .map(|(tag, item)| (*tag, self.edge(item, locals)))
                    .collect(),
                default: self.edge(default, locals),
            },
            Terminator::Trap { code } => Terminator::Trap { code: *code },
            Terminator::Return(opt) => self.complete(*opt, locals),
        };
        let cur = self.b.active;
        self.b.blocks[cur].exit = translated;
    }

    fn complete(&mut self, opt: Option<u32>, locals: &mut HashMap<u32, u32>) -> Terminator {
        if let Some(val) = opt {
            let evaluated = self.fetch(val, locals);
            let rptr = self.b.offset(self.frame, RESULT);
            self.b.store(rptr, evaluated, self.function.output.clone());
        }
        let nil = self.b.null();
        let cptr = self.b.offset(self.frame, CHILD);
        self.b.store(cptr, nil, Type::Pointer);
        let comp = self.b.unsigned(COMPLETED);
        let sptr = self.b.offset(self.frame, STATE);
        self.b.store(sptr, comp, UNSIGNED);
        let one = self.b.signed(1);
        Terminator::Return(Some(one))
    }

    fn edge(&mut self, item: &Target, locals: &mut HashMap<u32, u32>) -> Target {
        Target {
            block: self.map[&item.block],
            arguments: item
                .arguments
                .iter()
                .map(|&id| self.fetch(id, locals))
                .collect(),
        }
    }
}
