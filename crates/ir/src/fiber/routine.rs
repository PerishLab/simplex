use super::builder::{Builder, UNSIGNED};
use super::layout::{CANCEL, CANCELLED, CHILD, Plan, SIZE, STATE, STEP};
use super::runtime::{ABORT, SWEEP};
use super::{ALLOC, FREE, step};
use crate::{Binary, Function, Linkage, Terminator, Type};

pub fn transform(function: &Function) -> Vec<Function> {
    let plan = Plan::analyze(function);
    let stepname = format!("{}$step", function.symbol);
    let cancname = format!("{}$cancel", function.symbol);
    let ctor = constructor(function, &plan, (&stepname, &cancname));
    let worker = step::compile(function, &plan, &stepname);
    let drop = cancel(function, &plan, &cancname);
    vec![ctor, worker, drop]
}

fn constructor(function: &Function, plan: &Plan, names: (&str, &str)) -> Function {
    let mut b = Builder::new(
        function.symbol.clone(),
        function.linkage.clone(),
        Type::Pointer,
    );
    let mut incoming = Vec::with_capacity(function.inputs.len());
    for (_, kind) in &function.inputs {
        incoming.push((b.input(kind.clone()), kind.clone()));
    }
    let entry = b.block();
    b.select(entry);
    let total = b.unsigned(plan.total);
    let frame = b.call(ALLOC, vec![total], vec![UNSIGNED], Type::Pointer);
    let stepfn = b.symbol(names.0);
    let cancfn = b.symbol(names.1);
    let nil = b.null();
    let zero = b.unsigned(0);
    let pstep = b.offset(frame, STEP);
    b.store(pstep, stepfn, Type::Pointer);
    let pcanc = b.offset(frame, CANCEL);
    b.store(pcanc, cancfn, Type::Pointer);
    let pchild = b.offset(frame, CHILD);
    b.store(pchild, nil, Type::Pointer);
    let pstate = b.offset(frame, STATE);
    b.store(pstate, zero, UNSIGNED);
    let psize = b.offset(frame, SIZE);
    b.store(psize, total, UNSIGNED);
    let pbatch = b.offset(frame, plan.batch);
    b.store(pbatch, nil, Type::Pointer);
    let pcount = b.offset(frame, plan.count);
    b.store(pcount, zero, UNSIGNED);
    if let Some((off, _)) = plan.first {
        let pmask = b.offset(frame, off);
        b.store(pmask, zero, UNSIGNED);
    }
    for ((orig, _), (local, kind)) in function.inputs.iter().zip(incoming) {
        if let Some(&off) = plan.values.get(orig) {
            let dst = b.offset(frame, off);
            b.store(dst, local, kind);
        }
    }
    b.blocks[entry as usize].exit = Terminator::Return(Some(frame));
    b.finish()
}

struct Cleaner<'a> {
    b: &'a mut Builder,
    function: &'a Function,
    plan: &'a Plan,
    frame: u32,
}

fn cancel(function: &Function, plan: &Plan, name: &str) -> Function {
    let mut b = Builder::new(name.to_owned(), Linkage::Internal, Type::Void);
    let frame = b.input(Type::Pointer);
    let entry = b.block();
    let inspect = b.block();
    let active = b.block();
    let done = b.block();
    b.select(entry);
    let empty = b.isnull(frame);
    b.branch(empty, (done, Vec::new()), (inspect, Vec::new()));
    b.select(inspect);
    let stptr = b.offset(frame, STATE);
    let state = b.load(stptr, UNSIGNED);
    let limit = b.unsigned(CANCELLED);
    let settled = b.binary(Binary::Ge, state, limit, UNSIGNED);
    b.branch(settled, (done, Vec::new()), (active, Vec::new()));
    b.select(active);
    let mark = b.unsigned(CANCELLED);
    b.store(stptr, mark, UNSIGNED);
    let mut cleaner = Cleaner {
        b: &mut b,
        function,
        plan,
        frame,
    };
    cleaner.child();
    cleaner.batch();
    cleaner.hook();
    cleaner.release(done);
    b.select(done);
    b.blocks[done as usize].exit = Terminator::Return(None);
    b.finish()
}

impl Cleaner<'_> {
    fn child(&mut self) {
        let kill = self.b.block();
        let cont = self.b.block();
        let cptr = self.b.offset(self.frame, CHILD);
        let sub = self.b.load(cptr, Type::Pointer);
        let none = self.b.isnull(sub);
        self.b.branch(none, (cont, Vec::new()), (kill, Vec::new()));
        self.b.select(kill);
        let nil = self.b.null();
        self.b.store(cptr, nil, Type::Pointer);
        let _ = self
            .b
            .call(ABORT, vec![sub], vec![Type::Pointer], Type::Void);
        self.b.jump(cont, Vec::new());
        self.b.select(cont);
    }

    fn batch(&mut self) {
        let kill = self.b.block();
        let cont = self.b.block();
        let bptr = self.b.offset(self.frame, self.plan.batch);
        let arr = self.b.load(bptr, Type::Pointer);
        let none = self.b.isnull(arr);
        self.b.branch(none, (cont, Vec::new()), (kill, Vec::new()));
        self.b.select(kill);
        let cptr = self.b.offset(self.frame, self.plan.count);
        let len = self.b.load(cptr, UNSIGNED);
        let nil = self.b.null();
        let zero = self.b.unsigned(0);
        self.b.store(bptr, nil, Type::Pointer);
        self.b.store(cptr, zero, UNSIGNED);
        let _ = self.b.call(
            SWEEP,
            vec![arr, len],
            vec![Type::Pointer, UNSIGNED],
            Type::Void,
        );
        self.b.jump(cont, Vec::new());
        self.b.select(cont);
    }

    fn hook(&mut self) {
        let Some(symbol) = &self.function.cleanup else {
            return;
        };
        let arg = match self.plan.first {
            Some((off, _)) => self.b.offset(self.frame, off),
            None => self.frame,
        };
        let _ = self
            .b
            .call(symbol, vec![arg], vec![Type::Pointer], Type::Void);
    }

    fn release(&mut self, done: u32) {
        let szptr = self.b.offset(self.frame, SIZE);
        let size = self.b.load(szptr, UNSIGNED);
        let _ = self.b.call(
            FREE,
            vec![self.frame, size],
            vec![Type::Pointer, UNSIGNED],
            Type::Void,
        );
        self.b.jump(done, Vec::new());
    }
}
