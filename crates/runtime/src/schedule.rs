use crate::Syscall;
use crate::encoder::{Assembler, Condition, Register};
use crate::frame::*;

impl Assembler {
    pub fn release(&mut self) -> usize {
        let entry = self.label();
        let done = self.label();
        self.bind(entry);
        self.test(Register::Rdi);
        self.branch(Condition::Equal, done);
        self.load(Register::Rsi, Register::Rdi, SIZE);
        self.load(Register::Rdi, Register::Rdi, BASE);
        self.constant(Register::Rax, Syscall::Munmap as u32);
        self.system();
        self.bind(done);
        self.leave();
        entry
    }

    pub fn step(&mut self) -> usize {
        let entry = self.label();
        let back = self.label();
        let done = self.label();
        self.bind(entry);
        self.prologue(8);
        self.test(Register::Rdi);
        self.branch(Condition::Equal, done);
        self.check(Register::Rdi, STATE, READY);
        self.branch(Condition::Different, done);
        self.copy(Register::R12, Register::Rdi);
        self.current(Register::Rax);
        self.store(Register::R12, PREVIOUS, Register::Rax);
        self.install(Register::R12);
        self.put(Register::R12, STATE, RUNNING);
        self.save(Register::R12, PARENT);
        self.store(Register::R12, PARENT + RSP, Register::Rsp);
        self.anchor(Register::Rax, back);
        self.store(Register::R12, PARENT + RIP, Register::Rax);
        self.restore(FIBER);
        self.bind(back);
        self.bind(done);
        self.epilogue(8);
        entry
    }

    pub fn defer(&mut self) -> usize {
        let entry = self.label();
        let back = self.label();
        let done = self.label();
        self.bind(entry);
        self.prologue(8);
        self.copy(Register::R13, Register::Rdi);
        self.copy(Register::R14, Register::Rsi);
        self.current(Register::R12);
        self.test(Register::R12);
        self.branch(Condition::Equal, done);
        self.store(Register::R12, BATCH, Register::R13);
        self.store(Register::R12, COUNT, Register::R14);
        self.save(Register::R12, FIBER);
        self.store(Register::R12, FIBER + RSP, Register::Rsp);
        self.anchor(Register::Rax, back);
        self.store(Register::R12, FIBER + RIP, Register::Rax);
        self.put(Register::R12, STATE, 0);
        self.resume();
        self.bind(back);
        self.put(Register::R12, BATCH, 0);
        self.put(Register::R12, COUNT, 0);
        self.bind(done);
        self.epilogue(8);
        entry
    }

    fn nested(&mut self, entry: usize, drop: usize, clean: usize) {
        let scan = self.label();
        let abort = self.label();
        self.load(Register::R13, Register::R12, BATCH);
        self.load(Register::R14, Register::R12, COUNT);
        self.test(Register::R13);
        self.branch(Condition::Equal, clean);
        self.bind(scan);
        self.test(Register::R14);
        self.branch(Condition::Equal, clean);
        self.subtract(Register::R14, 1);
        self.indexed(Register::Rdi, Register::R13, Register::R14);
        self.test(Register::Rdi);
        self.branch(Condition::Equal, scan);
        self.check(Register::Rdi, STATE, COMPLETED);
        self.branch(Condition::Different, abort);
        self.invoke(drop);
        self.jump(scan);
        self.bind(abort);
        self.invoke(entry);
        self.jump(scan);
    }

    pub fn cancel(&mut self, drop: usize) -> usize {
        let entry = self.label();
        let clean = self.label();
        let free = self.label();
        let done = self.label();
        self.bind(entry);
        self.prologue(8);
        self.test(Register::Rdi);
        self.branch(Condition::Equal, done);
        self.check(Register::Rdi, STATE, COMPLETED);
        self.branch(Condition::Equal, done);
        self.check(Register::Rdi, STATE, CANCELLED);
        self.branch(Condition::Equal, done);
        self.copy(Register::R12, Register::Rdi);
        self.put(Register::R12, STATE, ABORTED);
        self.nested(entry, drop, clean);
        self.bind(clean);
        self.load(Register::Rax, Register::R12, CLEANUP);
        self.test(Register::Rax);
        self.branch(Condition::Equal, free);
        self.load(Register::Rdi, Register::R12, ENVIRONMENT);
        self.indirect(Register::Rax);
        self.bind(free);
        self.copy(Register::Rdi, Register::R12);
        self.invoke(drop);
        self.bind(done);
        self.zero(Register::Rax);
        self.epilogue(8);
        entry
    }
}

pub fn abort() -> Vec<u8> {
    let mut asm = Assembler::new();
    let start = asm.label();
    asm.jump(start);
    let drop = asm.release();
    let entry = asm.cancel(drop);
    asm.bind(start);
    asm.jump(entry);
    asm.finish()
}

pub fn wait() -> Vec<u8> {
    let mut asm = Assembler::new();
    let spin = asm.label();
    let ready = asm.label();
    let empty = asm.label();
    asm.prologue(24);
    asm.test(Register::Rdi);
    asm.branch(Condition::Equal, empty);
    asm.copy(Register::R12, Register::Rdi);
    asm.store(Register::Rsp, 0, Register::R12);
    let pause = asm.label();
    let advance = asm.label();
    let drop = asm.label();
    asm.bind(spin);
    asm.check(Register::R12, STATE, COMPLETED);
    asm.branch(Condition::Equal, ready);
    asm.check(Register::R12, STATE, CANCELLED);
    asm.branch(Condition::Equal, empty);
    asm.copy(Register::Rdi, Register::Rsp);
    asm.constant(Register::Rsi, 1);
    asm.invoke(pause);
    asm.copy(Register::Rdi, Register::R12);
    asm.invoke(advance);
    asm.jump(spin);
    asm.bind(ready);
    asm.load(Register::R13, Register::R12, LOW);
    asm.load(Register::R14, Register::R12, HIGH);
    asm.copy(Register::Rdi, Register::R12);
    asm.invoke(drop);
    asm.copy(Register::Rax, Register::R13);
    asm.copy(Register::Rdx, Register::R14);
    asm.epilogue(24);
    asm.bind(empty);
    asm.zero(Register::Rax);
    asm.zero(Register::Rdx);
    asm.epilogue(24);
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
