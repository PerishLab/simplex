use crate::encoder::{Assembler, Condition, Register};
use crate::frame::*;
use crate::{GUARD, Syscall};

impl Assembler {
    pub fn prologue(&mut self, reserve: i8) {
        self.push(Register::Rbp);
        self.copy(Register::Rbp, Register::Rsp);
        self.push(Register::Rbx);
        self.push(Register::R12);
        self.push(Register::R13);
        self.push(Register::R14);
        self.push(Register::R15);
        self.subtract(Register::Rsp, reserve);
    }

    pub fn epilogue(&mut self, reserve: i8) {
        self.add(Register::Rsp, reserve);
        self.pop(Register::R15);
        self.pop(Register::R14);
        self.pop(Register::R13);
        self.pop(Register::R12);
        self.pop(Register::Rbx);
        self.pop(Register::Rbp);
        self.leave();
    }

    pub fn terminate(&mut self) {
        self.constant(Register::Rax, GROUP);
        self.system();
        self.constant(Register::Rax, Syscall::Exit as u32);
        self.system();
        self.trap();
    }

    pub fn current(&mut self, dst: Register) {
        self.constant(Register::Rax, PRCTL);
        self.constant(Register::Rdi, QUERY);
        self.copy(Register::Rsi, Register::Rsp);
        self.system();
        self.load(dst, Register::Rsp, 0);
    }

    pub fn install(&mut self, src: Register) {
        self.constant(Register::Rax, PRCTL);
        self.constant(Register::Rdi, ASSIGN);
        self.copy(Register::Rsi, src);
        self.system();
    }

    pub fn save(&mut self, base: Register, region: i32) {
        self.store(base, region + RBX, Register::Rbx);
        self.store(base, region + RBP, Register::Rbp);
        self.store(base, region + R12, Register::R12);
        self.store(base, region + R13, Register::R13);
        self.store(base, region + R14, Register::R14);
        self.store(base, region + R15, Register::R15);
    }

    pub fn restore(&mut self, region: i32) {
        self.load(Register::Rbx, Register::R12, region + RBX);
        self.load(Register::Rbp, Register::R12, region + RBP);
        self.load(Register::R13, Register::R12, region + R13);
        self.load(Register::R14, Register::R12, region + R14);
        self.load(Register::R15, Register::R12, region + R15);
        self.load(Register::Rsp, Register::R12, region + RSP);
        self.load(Register::Rcx, Register::R12, region + RIP);
        self.load(Register::R12, Register::R12, region + R12);
        self.zero(Register::Rax);
        self.transfer(Register::Rcx);
    }

    pub fn resume(&mut self) {
        self.load(Register::Rsi, Register::R12, PREVIOUS);
        self.constant(Register::Rax, PRCTL);
        self.constant(Register::Rdi, ASSIGN);
        self.system();
        self.restore(PARENT);
    }

    fn allocate(&mut self, fail: usize) {
        self.zero(Register::Rdi);
        self.constant(Register::Rsi, TOTAL);
        self.constant(Register::Rdx, WRITABLE);
        self.constant(Register::R10, ANONYMOUS);
        self.signed(Register::R8, -1);
        self.zero(Register::R9);
        self.constant(Register::Rax, Syscall::Mmap as u32);
        self.system();
        self.limit(Register::Rax, THRESHOLD);
        self.branch(Condition::Limit, fail);
        self.copy(Register::Rbx, Register::Rax);
        self.copy(Register::Rdi, Register::Rbx);
        self.constant(Register::Rsi, GUARD as u32);
        self.zero(Register::Rdx);
        self.constant(Register::Rax, Syscall::Mprotect as u32);
        self.system();
        self.test(Register::Rax);
        self.branch(Condition::Different, fail);
    }

    fn populate(&mut self, runner: usize) {
        self.address(Register::R12, Register::Rbx, CONTROL);
        self.put(Register::R12, STATE, 0);
        self.store(Register::R12, BASE, Register::Rbx);
        self.put(Register::R12, SIZE, TOTAL as i32);
        self.store(Register::R12, ENTRY, Register::R13);
        self.store(Register::R12, ENVIRONMENT, Register::R14);
        self.store(Register::R12, CLEANUP, Register::R15);
        self.store(Register::R12, FIBER + R12, Register::R12);
        self.address(Register::Rax, Register::Rbx, TOP);
        self.store(Register::R12, FIBER + RSP, Register::Rax);
        self.anchor(Register::Rax, runner);
        self.store(Register::R12, FIBER + RIP, Register::Rax);
    }
}

pub fn start() -> Vec<u8> {
    let mut asm = Assembler::new();
    let empty = asm.label();
    asm.prologue(8);
    asm.copy(Register::Rbx, Register::Rdi);
    asm.zero(Register::Rsi);
    asm.install(Register::Rsi);
    asm.test(Register::Rbx);
    asm.branch(Condition::Equal, empty);
    asm.indirect(Register::Rbx);
    asm.copy(Register::Rdi, Register::Rax);
    asm.terminate();
    asm.bind(empty);
    asm.zero(Register::Rax);
    asm.epilogue(8);
    asm.finish()
}

pub fn spawn() -> Vec<u8> {
    let mut asm = Assembler::new();
    let fail = asm.label();
    let runner = asm.label();
    asm.prologue(8);
    asm.copy(Register::R13, Register::Rdi);
    asm.copy(Register::R14, Register::Rsi);
    asm.copy(Register::R15, Register::Rdx);
    asm.allocate(fail);
    asm.populate(runner);
    asm.copy(Register::Rax, Register::R12);
    asm.epilogue(8);
    asm.bind(fail);
    asm.constant(Register::Rdi, PANIC);
    asm.terminate();
    asm.bind(runner);
    asm.zero(Register::Rbp);
    asm.load(Register::Rdi, Register::R12, ENVIRONMENT);
    asm.load(Register::Rax, Register::R12, ENTRY);
    asm.indirect(Register::Rax);
    asm.store(Register::R12, LOW, Register::Rax);
    asm.store(Register::R12, HIGH, Register::Rdx);
    asm.put(Register::R12, STATE, DONE);
    asm.resume();
    asm.finish()
}

pub fn pause() -> Vec<u8> {
    let mut asm = Assembler::new();
    let done = asm.label();
    asm.subtract(Register::Rsp, 16);
    asm.current(Register::Rdi);
    asm.add(Register::Rsp, 16);
    asm.test(Register::Rdi);
    asm.branch(Condition::Equal, done);
    asm.save(Register::Rdi, FIBER);
    asm.address(Register::Rax, Register::Rsp, 8);
    asm.store(Register::Rdi, FIBER + RSP, Register::Rax);
    asm.load(Register::Rax, Register::Rsp, 0);
    asm.store(Register::Rdi, FIBER + RIP, Register::Rax);
    asm.put(Register::Rdi, STATE, 0);
    asm.copy(Register::R12, Register::Rdi);
    asm.resume();
    asm.bind(done);
    asm.zero(Register::Rax);
    asm.leave();
    asm.finish()
}
