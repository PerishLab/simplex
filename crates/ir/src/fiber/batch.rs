use super::ALLOC;
use super::builder::{BYTE, Builder, SIGNED, UNSIGNED};
use super::layout::{CANCELLED, COMPLETED, RESULT, STATE};
use super::runtime::{DROP, SELECT, SWEEP, TICK};
use crate::{Binary, Linkage, Operation, Terminator, Type};

pub const ROUND: &str = "sx$coro$round";
pub const PACK: &str = "sx$coro$pack";
pub const CONTEST: &str = "sx$coro$contest";
pub const GATHER: &str = "sx$coro$gather";
pub const RACE: &str = "sx$coro$race";

pub fn round() -> crate::Function {
    let mut b = Builder::new(ROUND.to_owned(), Linkage::Internal, SIGNED);
    let fibers = b.input(Type::Pointer);
    let count = b.input(UNSIGNED);
    let entry = b.block();
    let head = b.block();
    let body = b.block();
    let poll = b.block();
    let next = b.block();
    let done = b.block();
    let idx = b.parameter(head, UNSIGNED);
    let pending = b.parameter(head, UNSIGNED);
    let steppend = b.parameter(next, UNSIGNED);
    b.select(entry);
    let zero = b.unsigned(0);
    b.jump(head, vec![zero, zero]);
    b.select(head);
    let more = b.binary(Binary::Lt, idx, count, UNSIGNED);
    b.branch(more, (body, Vec::new()), (done, Vec::new()));
    b.select(body);
    let eight = b.unsigned(8);
    let off = b.binary(Binary::Mul, idx, eight, UNSIGNED);
    let slot = b.advance(fibers, off);
    let item = b.load(slot, Type::Pointer);
    let empty = b.isnull(item);
    b.branch(empty, (next, vec![pending]), (poll, Vec::new()));
    b.select(poll);
    let ready = b.call(TICK, vec![item], vec![Type::Pointer], SIGNED);
    let zeroi = b.signed(0);
    let waited = b.binary(Binary::Eq, ready, zeroi, SIGNED);
    let one = b.unsigned(1);
    let added = b.binary(Binary::Add, pending, one, UNSIGNED);
    b.branch(waited, (next, vec![added]), (next, vec![pending]));
    b.select(next);
    let stepidx = b.unsigned(1);
    let nextidx = b.binary(Binary::Add, idx, stepidx, UNSIGNED);
    b.jump(head, vec![nextidx, steppend]);
    b.select(done);
    let z = b.unsigned(0);
    let all = b.binary(Binary::Eq, pending, z, UNSIGNED);
    let yes = b.block();
    let no = b.block();
    b.branch(all, (yes, Vec::new()), (no, Vec::new()));
    b.select(yes);
    let r1 = b.signed(1);
    b.blocks[yes as usize].exit = Terminator::Return(Some(r1));
    b.select(no);
    let r0 = b.signed(0);
    b.blocks[no as usize].exit = Terminator::Return(Some(r0));
    b.finish()
}

pub fn pack() -> crate::Function {
    let mut b = Builder::new(PACK.to_owned(), Linkage::Internal, Type::Pair);
    let fibers = b.input(Type::Pointer);
    let count = b.input(UNSIGNED);
    let bytes = b.input(UNSIGNED);
    let entry = b.block();
    let alloc = b.block();
    let head = b.block();
    let body = b.block();
    let copy = b.block();
    let word = b.block();
    let free = b.block();
    let done = b.block();
    let stride = b.parameter(alloc, UNSIGNED);
    let idx = b.parameter(head, UNSIGNED);
    let cursor = b.parameter(copy, UNSIGNED);
    b.select(entry);
    let zero = b.unsigned(0);
    let one = b.unsigned(1);
    let iszero = b.binary(Binary::Eq, bytes, zero, UNSIGNED);
    b.branch(iszero, (alloc, vec![one]), (alloc, vec![bytes]));
    b.select(alloc);
    let raw = b.binary(Binary::Mul, count, stride, UNSIGNED);
    let eight = b.unsigned(8);
    let small = b.binary(Binary::Lt, raw, eight, UNSIGNED);
    let sizeblk = b.block();
    let size = b.parameter(sizeblk, UNSIGNED);
    b.branch(small, (sizeblk, vec![eight]), (sizeblk, vec![raw]));
    b.select(sizeblk);
    let out = b.call(ALLOC, vec![size], vec![UNSIGNED], Type::Pointer);
    let start = b.unsigned(0);
    b.jump(head, vec![start]);
    b.select(head);
    let more = b.binary(Binary::Lt, idx, count, UNSIGNED);
    b.branch(more, (body, Vec::new()), (done, Vec::new()));
    b.select(body);
    let w8 = b.unsigned(8);
    let foff = b.binary(Binary::Mul, idx, w8, UNSIGNED);
    let fslot = b.advance(fibers, foff);
    let item = b.load(fslot, Type::Pointer);
    let step1 = b.unsigned(1);
    let nextidx = b.binary(Binary::Add, idx, step1, UNSIGNED);
    let empty = b.isnull(item);
    let prep = b.block();
    b.branch(empty, (head, vec![nextidx]), (prep, Vec::new()));
    b.select(prep);
    let nil = b.null();
    b.store(fslot, nil, Type::Pointer);
    let doff = b.binary(Binary::Mul, idx, stride, UNSIGNED);
    let dst = b.advance(out, doff);
    let src = b.offset(item, RESULT);
    let c0 = b.unsigned(0);
    b.jump(copy, vec![c0]);
    b.select(copy);
    let rem = b.binary(Binary::Lt, cursor, bytes, UNSIGNED);
    b.branch(rem, (word, Vec::new()), (free, Vec::new()));
    b.select(word);
    let sa = b.advance(src, cursor);
    let da = b.advance(dst, cursor);
    let val = b.load(sa, BYTE);
    b.store(da, val, BYTE);
    let inc = b.unsigned(1);
    let nextcur = b.binary(Binary::Add, cursor, inc, UNSIGNED);
    b.jump(copy, vec![nextcur]);
    b.select(free);
    let _ = b.call(DROP, vec![item], vec![Type::Pointer], Type::Void);
    b.jump(head, vec![nextidx]);
    b.select(done);
    let pair = b.emit(
        Type::Pair,
        Operation::Pair {
            low: out,
            high: count,
        },
    );
    b.blocks[done as usize].exit = Terminator::Return(Some(pair));
    b.finish()
}

pub fn contest() -> crate::Function {
    let mut b = Builder::new(CONTEST.to_owned(), Linkage::Internal, SIGNED);
    let fibers = b.input(Type::Pointer);
    let count = b.input(UNSIGNED);
    let entry = b.block();
    let head = b.block();
    let body = b.block();
    let inspect = b.block();
    let poll = b.block();
    let won = b.block();
    let next = b.block();
    let done = b.block();
    let idx = b.parameter(head, UNSIGNED);
    let active = b.parameter(head, UNSIGNED);
    let nextact = b.parameter(next, UNSIGNED);
    b.select(entry);
    let zero = b.unsigned(0);
    b.jump(head, vec![zero, zero]);
    b.select(head);
    let more = b.binary(Binary::Lt, idx, count, UNSIGNED);
    b.branch(more, (body, Vec::new()), (done, Vec::new()));
    b.select(body);
    let eight = b.unsigned(8);
    let off = b.binary(Binary::Mul, idx, eight, UNSIGNED);
    let slot = b.advance(fibers, off);
    let item = b.load(slot, Type::Pointer);
    let empty = b.isnull(item);
    b.branch(empty, (next, vec![active]), (inspect, Vec::new()));
    b.select(inspect);
    let stptr = b.offset(item, STATE);
    let state = b.load(stptr, UNSIGNED);
    let comp = b.unsigned(COMPLETED);
    let already = b.binary(Binary::Eq, state, comp, UNSIGNED);
    let alive = b.block();
    b.branch(already, (won, Vec::new()), (alive, Vec::new()));
    b.select(alive);
    let canc = b.unsigned(CANCELLED);
    let skipped = b.binary(Binary::Eq, state, canc, UNSIGNED);
    b.branch(skipped, (next, vec![active]), (poll, Vec::new()));
    b.select(poll);
    let ready = b.call(TICK, vec![item], vec![Type::Pointer], SIGNED);
    let z = b.signed(0);
    let hit = b.binary(Binary::Ne, ready, z, SIGNED);
    let one = b.unsigned(1);
    let bumped = b.binary(Binary::Add, active, one, UNSIGNED);
    b.branch(hit, (won, Vec::new()), (next, vec![bumped]));
    b.select(won);
    let ans = b.bitcast(idx, UNSIGNED, SIGNED);
    b.blocks[won as usize].exit = Terminator::Return(Some(ans));
    b.select(next);
    let inc = b.unsigned(1);
    let nextidx = b.binary(Binary::Add, idx, inc, UNSIGNED);
    b.jump(head, vec![nextidx, nextact]);
    b.select(done);
    let z0 = b.unsigned(0);
    let none = b.binary(Binary::Eq, active, z0, UNSIGNED);
    let dead = b.block();
    let wait = b.block();
    b.branch(none, (dead, Vec::new()), (wait, Vec::new()));
    b.select(dead);
    let m2 = b.signed(-2);
    b.blocks[dead as usize].exit = Terminator::Return(Some(m2));
    b.select(wait);
    let m1 = b.signed(-1);
    b.blocks[wait as usize].exit = Terminator::Return(Some(m1));
    b.finish()
}

pub fn gather() -> crate::Function {
    let mut b = Builder::new(GATHER.to_owned(), Linkage::Internal, Type::Pair);
    let fibers = b.input(Type::Pointer);
    let count = b.input(UNSIGNED);
    let bytes = b.input(UNSIGNED);
    let head = b.block();
    let done = b.block();
    b.select(head);
    let ready = b.call(
        ROUND,
        vec![fibers, count],
        vec![Type::Pointer, UNSIGNED],
        SIGNED,
    );
    let zero = b.signed(0);
    let ok = b.binary(Binary::Ne, ready, zero, SIGNED);
    b.branch(ok, (done, Vec::new()), (head, Vec::new()));
    b.select(done);
    let pair = b.call(
        PACK,
        vec![fibers, count, bytes],
        vec![Type::Pointer, UNSIGNED, UNSIGNED],
        Type::Pair,
    );
    b.blocks[done as usize].exit = Terminator::Return(Some(pair));
    b.finish()
}

pub fn race() -> crate::Function {
    let mut b = Builder::new(RACE.to_owned(), Linkage::Internal, Type::Void);
    let fibers = b.input(Type::Pointer);
    let count = b.input(UNSIGNED);
    let out = b.input(Type::Pointer);
    let bytes = b.input(UNSIGNED);
    let entry = b.block();
    let copy = b.block();
    let loopblk = b.block();
    let step = b.block();
    let clean = b.block();
    let cursor = b.parameter(loopblk, UNSIGNED);
    b.select(entry);
    let winner = b.call(
        SELECT,
        vec![fibers, count],
        vec![Type::Pointer, UNSIGNED],
        SIGNED,
    );
    let zero = b.signed(0);
    let valid = b.binary(Binary::Ge, winner, zero, SIGNED);
    b.branch(valid, (copy, Vec::new()), (clean, Vec::new()));
    b.select(copy);
    let idx = b.bitcast(winner, SIGNED, UNSIGNED);
    let eight = b.unsigned(8);
    let off = b.binary(Binary::Mul, idx, eight, UNSIGNED);
    let slot = b.advance(fibers, off);
    let item = b.load(slot, Type::Pointer);
    let src = b.offset(item, RESULT);
    let c0 = b.unsigned(0);
    b.jump(loopblk, vec![c0]);
    b.select(loopblk);
    let more = b.binary(Binary::Lt, cursor, bytes, UNSIGNED);
    b.branch(more, (step, Vec::new()), (clean, Vec::new()));
    b.select(step);
    let sa = b.advance(src, cursor);
    let da = b.advance(out, cursor);
    let val = b.load(sa, BYTE);
    b.store(da, val, BYTE);
    let one = b.unsigned(1);
    let next = b.binary(Binary::Add, cursor, one, UNSIGNED);
    b.jump(loopblk, vec![next]);
    b.select(clean);
    let _ = b.call(
        SWEEP,
        vec![fibers, count],
        vec![Type::Pointer, UNSIGNED],
        Type::Void,
    );
    b.blocks[clean as usize].exit = Terminator::Return(None);
    b.finish()
}
