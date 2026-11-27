use super::FREE;
use super::batch::{CONTEST, GATHER, PACK, RACE, ROUND, contest, gather, pack, race, round};
use super::builder::{Builder, SIGNED, UNSIGNED};
use super::layout::{CANCEL, CANCELLED, COMPLETED, SIZE, STATE, STEP};
use crate::{Binary, Linkage, Terminator, Type};

pub const TICK: &str = "sx$coro$tick";
pub const DROP: &str = "sx$coro$drop";
pub const ABORT: &str = "sx$coro$abort";
pub const SWEEP: &str = "sx$coro$sweep";
pub const DRIVE: &str = "sx$coro$drive";
pub const SELECT: &str = "sx$coro$select";

pub fn names() -> [&'static str; 5] {
    [ROUND, PACK, CONTEST, GATHER, RACE]
}

pub fn synthesize() -> Vec<crate::Function> {
    let _ = names();
    vec![
        tick(),
        drop(),
        abort(),
        sweep(),
        round(),
        pack(),
        contest(),
        drive(),
        select(),
        gather(),
        race(),
    ]
}

fn tick() -> crate::Function {
    let mut b = Builder::new(TICK.to_owned(), Linkage::Internal, SIGNED);
    let fiber = b.input(Type::Pointer);
    let entry = b.block();
    let check = b.block();
    let run = b.block();
    let done = b.block();
    b.select(entry);
    let empty = b.isnull(fiber);
    b.branch(empty, (done, Vec::new()), (check, Vec::new()));
    b.select(check);
    let stptr = b.offset(fiber, STATE);
    let state = b.load(stptr, UNSIGNED);
    let limit = b.unsigned(CANCELLED);
    let settled = b.binary(Binary::Ge, state, limit, UNSIGNED);
    b.branch(settled, (done, Vec::new()), (run, Vec::new()));
    b.select(run);
    let fnptr = b.offset(fiber, STEP);
    let step = b.load(fnptr, Type::Pointer);
    let res = b.indirect(step, vec![fiber], vec![Type::Pointer], SIGNED);
    b.blocks[run as usize].exit = Terminator::Return(Some(res));
    b.select(done);
    let one = b.signed(1);
    b.blocks[done as usize].exit = Terminator::Return(Some(one));
    b.finish()
}

fn drop() -> crate::Function {
    let mut b = Builder::new(DROP.to_owned(), Linkage::Internal, Type::Void);
    let fiber = b.input(Type::Pointer);
    let entry = b.block();
    let live = b.block();
    let done = b.block();
    b.select(entry);
    let empty = b.isnull(fiber);
    b.branch(empty, (done, Vec::new()), (live, Vec::new()));
    b.select(live);
    let szptr = b.offset(fiber, SIZE);
    let size = b.load(szptr, UNSIGNED);
    let _ = b.call(
        FREE,
        vec![fiber, size],
        vec![Type::Pointer, UNSIGNED],
        Type::Void,
    );
    b.jump(done, Vec::new());
    b.select(done);
    b.blocks[done as usize].exit = Terminator::Return(None);
    b.finish()
}

fn abort() -> crate::Function {
    let mut b = Builder::new(ABORT.to_owned(), Linkage::Internal, Type::Void);
    let fiber = b.input(Type::Pointer);
    let entry = b.block();
    let inspect = b.block();
    let release = b.block();
    let check = b.block();
    let invoke = b.block();
    let done = b.block();
    b.select(entry);
    let empty = b.isnull(fiber);
    b.branch(empty, (done, Vec::new()), (inspect, Vec::new()));
    b.select(inspect);
    let stptr = b.offset(fiber, STATE);
    let state = b.load(stptr, UNSIGNED);
    let comp = b.unsigned(COMPLETED);
    let iscomp = b.binary(Binary::Eq, state, comp, UNSIGNED);
    b.branch(iscomp, (release, Vec::new()), (check, Vec::new()));
    b.select(release);
    let _ = b.call(DROP, vec![fiber], vec![Type::Pointer], Type::Void);
    b.jump(done, Vec::new());
    b.select(check);
    let canc = b.unsigned(CANCELLED);
    let iscanc = b.binary(Binary::Eq, state, canc, UNSIGNED);
    b.branch(iscanc, (done, Vec::new()), (invoke, Vec::new()));
    b.select(invoke);
    let fnptr = b.offset(fiber, CANCEL);
    let cancel = b.load(fnptr, Type::Pointer);
    let _ = b.indirect(cancel, vec![fiber], vec![Type::Pointer], Type::Void);
    b.jump(done, Vec::new());
    b.select(done);
    b.blocks[done as usize].exit = Terminator::Return(None);
    b.finish()
}

fn sweep() -> crate::Function {
    let mut b = Builder::new(SWEEP.to_owned(), Linkage::Internal, Type::Void);
    let fibers = b.input(Type::Pointer);
    let count = b.input(UNSIGNED);
    let entry = b.block();
    let head = b.block();
    let body = b.block();
    let settle = b.block();
    let done = b.block();
    let rem = b.parameter(head, UNSIGNED);
    b.select(entry);
    b.jump(head, vec![count]);
    b.select(head);
    let zero = b.unsigned(0);
    let stop = b.binary(Binary::Eq, rem, zero, UNSIGNED);
    b.branch(stop, (done, Vec::new()), (body, Vec::new()));
    b.select(body);
    let one = b.unsigned(1);
    let idx = b.binary(Binary::Sub, rem, one, UNSIGNED);
    let eight = b.unsigned(8);
    let off = b.binary(Binary::Mul, idx, eight, UNSIGNED);
    let slot = b.advance(fibers, off);
    let item = b.load(slot, Type::Pointer);
    let empty = b.isnull(item);
    b.branch(empty, (head, vec![idx]), (settle, Vec::new()));
    b.select(settle);
    let nil = b.null();
    b.store(slot, nil, Type::Pointer);
    let _ = b.call(ABORT, vec![item], vec![Type::Pointer], Type::Void);
    b.jump(head, vec![idx]);
    b.select(done);
    b.blocks[done as usize].exit = Terminator::Return(None);
    b.finish()
}

fn drive() -> crate::Function {
    let mut b = Builder::new(DRIVE.to_owned(), Linkage::Internal, Type::Void);
    let fiber = b.input(Type::Pointer);
    let head = b.block();
    let done = b.block();
    b.select(head);
    let ready = b.call(TICK, vec![fiber], vec![Type::Pointer], SIGNED);
    let zero = b.signed(0);
    let finished = b.binary(Binary::Ne, ready, zero, SIGNED);
    b.branch(finished, (done, Vec::new()), (head, Vec::new()));
    b.select(done);
    b.blocks[done as usize].exit = Terminator::Return(None);
    b.finish()
}

fn select() -> crate::Function {
    let mut b = Builder::new(SELECT.to_owned(), Linkage::Internal, SIGNED);
    let fibers = b.input(Type::Pointer);
    let count = b.input(UNSIGNED);
    let head = b.block();
    let done = b.block();
    b.select(head);
    let res = b.call(
        CONTEST,
        vec![fibers, count],
        vec![Type::Pointer, UNSIGNED],
        SIGNED,
    );
    let m1 = b.signed(-1);
    let pending = b.binary(Binary::Eq, res, m1, SIGNED);
    b.branch(pending, (head, Vec::new()), (done, Vec::new()));
    b.select(done);
    b.blocks[done as usize].exit = Terminator::Return(Some(res));
    b.finish()
}
