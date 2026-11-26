use crate::encoder::{Assembler, Condition, Register};
use crate::frame::*;

impl Assembler {
    fn poll(&mut self, advance: usize) {
        let scan = self.label();
        let next = self.label();
        let done = self.label();
        self.zero(Register::R15);
        self.zero(Register::Rbx);
        self.bind(scan);
        self.compare(Register::Rbx, Register::R13);
        self.branch(Condition::Limit, done);
        self.indexed(Register::Rdi, Register::R12, Register::Rbx);
        self.test(Register::Rdi);
        self.branch(Condition::Equal, next);
        self.check(Register::Rdi, STATE, READY);
        self.branch(Condition::Different, next);
        self.invoke(advance);
        self.indexed(Register::Rdi, Register::R12, Register::Rbx);
        self.check(Register::Rdi, STATE, READY);
        self.branch(Condition::Different, next);
        self.add(Register::R15, 1);
        self.bind(next);
        self.add(Register::Rbx, 1);
        self.jump(scan);
        self.bind(done);
    }

    fn collect(&mut self, drop: usize) {
        let scan = self.label();
        let free = self.label();
        let next = self.label();
        let done = self.label();
        self.zero(Register::Rbx);
        self.bind(scan);
        self.compare(Register::Rbx, Register::R13);
        self.branch(Condition::Limit, done);
        self.indexed(Register::Rdi, Register::R12, Register::Rbx);
        self.test(Register::Rdi);
        self.branch(Condition::Equal, next);
        self.test(Register::R14);
        self.branch(Condition::Equal, free);
        self.copy(Register::Rax, Register::Rbx);
        self.shift(Register::Rax, 4);
        self.plus(Register::Rax, Register::R14);
        self.load(Register::Rcx, Register::Rdi, LOW);
        self.store(Register::Rax, 0, Register::Rcx);
        self.load(Register::Rcx, Register::Rdi, HIGH);
        self.store(Register::Rax, 8, Register::Rcx);
        self.bind(free);
        self.invoke(drop);
        self.bind(next);
        self.add(Register::Rbx, 1);
        self.jump(scan);
        self.bind(done);
    }

    fn contest(&mut self, advance: usize, won: usize) {
        let scan = self.label();
        let next = self.label();
        let done = self.label();
        self.zero(Register::R15);
        self.zero(Register::Rbx);
        self.bind(scan);
        self.compare(Register::Rbx, Register::R13);
        self.branch(Condition::Limit, done);
        self.indexed(Register::Rdi, Register::R12, Register::Rbx);
        self.test(Register::Rdi);
        self.branch(Condition::Equal, next);
        self.check(Register::Rdi, STATE, COMPLETED);
        self.branch(Condition::Equal, won);
        self.check(Register::Rdi, STATE, READY);
        self.branch(Condition::Different, next);
        self.invoke(advance);
        self.indexed(Register::Rdi, Register::R12, Register::Rbx);
        self.check(Register::Rdi, STATE, COMPLETED);
        self.branch(Condition::Equal, won);
        self.check(Register::Rdi, STATE, READY);
        self.branch(Condition::Different, next);
        self.add(Register::R15, 1);
        self.bind(next);
        self.add(Register::Rbx, 1);
        self.jump(scan);
        self.bind(done);
    }

    fn settle(&mut self, abort: usize, drop: usize) {
        let scan = self.label();
        let loser = self.label();
        let done = self.label();
        self.copy(Register::R14, Register::R13);
        self.bind(scan);
        self.test(Register::R14);
        self.branch(Condition::Equal, done);
        self.subtract(Register::R14, 1);
        self.compare(Register::R14, Register::Rbx);
        self.branch(Condition::Equal, scan);
        self.indexed(Register::Rdi, Register::R12, Register::R14);
        self.test(Register::Rdi);
        self.branch(Condition::Equal, scan);
        self.check(Register::Rdi, STATE, COMPLETED);
        self.branch(Condition::Different, loser);
        self.invoke(drop);
        self.jump(scan);
        self.bind(loser);
        self.invoke(abort);
        self.jump(scan);
        self.bind(done);
        self.indexed(Register::Rdi, Register::R12, Register::Rbx);
        self.invoke(drop);
    }
}

pub fn all() -> Vec<u8> {
    let mut asm = Assembler::new();
    let round = asm.label();
    let finish = asm.label();
    let pause = asm.label();
    let advance = asm.label();
    let drop = asm.label();
    asm.prologue(8);
    asm.copy(Register::R12, Register::Rdi);
    asm.copy(Register::R13, Register::Rsi);
    asm.copy(Register::R14, Register::Rdx);
    asm.test(Register::R13);
    asm.branch(Condition::Equal, finish);
    asm.bind(round);
    asm.copy(Register::Rdi, Register::R12);
    asm.copy(Register::Rsi, Register::R13);
    asm.invoke(pause);
    asm.poll(advance);
    asm.test(Register::R15);
    asm.branch(Condition::Different, round);
    asm.collect(drop);
    asm.bind(finish);
    asm.copy(Register::Rax, Register::R14);
    asm.epilogue(8);
    let first = asm.defer();
    let second = asm.step();
    let third = asm.release();
    asm.bind(pause);
    asm.jump(first);
    asm.bind(advance);
    asm.jump(second);
    asm.bind(drop);
    asm.jump(third);
    asm.finish()
}

pub fn race() -> Vec<u8> {
    let mut asm = Assembler::new();
    let round = asm.label();
    let won = asm.label();
    let empty = asm.label();
    let pause = asm.label();
    let advance = asm.label();
    let drop = asm.label();
    let abort = asm.label();
    asm.prologue(24);
    asm.copy(Register::R12, Register::Rdi);
    asm.copy(Register::R13, Register::Rsi);
    asm.test(Register::R13);
    asm.branch(Condition::Equal, empty);
    asm.bind(round);
    asm.copy(Register::Rdi, Register::R12);
    asm.copy(Register::Rsi, Register::R13);
    asm.invoke(pause);
    asm.contest(advance, won);
    asm.test(Register::R15);
    asm.branch(Condition::Different, round);
    asm.jump(empty);
    asm.bind(won);
    asm.load(Register::Rax, Register::Rdi, LOW);
    asm.store(Register::Rsp, 0, Register::Rax);
    asm.load(Register::Rax, Register::Rdi, HIGH);
    asm.store(Register::Rsp, 8, Register::Rax);
    asm.settle(abort, drop);
    asm.load(Register::Rax, Register::Rsp, 0);
    asm.load(Register::Rdx, Register::Rsp, 8);
    asm.epilogue(24);
    asm.bind(empty);
    asm.zero(Register::Rax);
    asm.zero(Register::Rdx);
    asm.epilogue(24);
    let first = asm.defer();
    let second = asm.step();
    let third = asm.release();
    let fourth = asm.cancel(third);
    asm.bind(pause);
    asm.jump(first);
    asm.bind(advance);
    asm.jump(second);
    asm.bind(drop);
    asm.jump(third);
    asm.bind(abort);
    asm.jump(fourth);
    asm.finish()
}
