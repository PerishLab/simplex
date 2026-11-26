mod batch;
mod encoder;
mod fiber;
mod frame;
mod schedule;

use encoder::{Assembler, Register};

pub const STACK: usize = 65_536;
pub const GUARD: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Syscall {
    Read = 0,
    Write = 1,
    Mmap = 9,
    Mprotect = 10,
    Munmap = 11,
    Exit = 60,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    Start,
    Exit,
    Write,
    Spawn,
    Yield,
    Await,
    All,
    Race,
    Cancel,
}

impl Hook {
    pub const ALL: [Self; 9] = [
        Self::Start,
        Self::Exit,
        Self::Write,
        Self::Spawn,
        Self::Yield,
        Self::Await,
        Self::All,
        Self::Race,
        Self::Cancel,
    ];

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Start => "sx$rt$start",
            Self::Exit => "sx$rt$exit",
            Self::Write => "sx$rt$write",
            Self::Spawn => "sx$rt$spawn",
            Self::Yield => "sx$rt$yield",
            Self::Await => "sx$rt$await",
            Self::All => "sx$rt$all",
            Self::Race => "sx$rt$race",
            Self::Cancel => "sx$rt$cancel",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Context {
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rsp: u64,
    pub rip: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stub {
    pub hook: Hook,
    pub code: Vec<u8>,
}

pub fn start() -> Stub {
    Stub {
        hook: Hook::Start,
        code: fiber::start(),
    }
}

pub fn exit() -> Stub {
    let mut asm = Assembler::new();
    asm.terminate();
    Stub {
        hook: Hook::Exit,
        code: asm.finish(),
    }
}

pub fn write() -> Stub {
    let mut asm = Assembler::new();
    asm.constant(Register::Rax, Syscall::Write as u32);
    asm.system();
    asm.leave();
    Stub {
        hook: Hook::Write,
        code: asm.finish(),
    }
}

pub fn spawn() -> Stub {
    Stub {
        hook: Hook::Spawn,
        code: fiber::spawn(),
    }
}

pub fn pause() -> Stub {
    Stub {
        hook: Hook::Yield,
        code: fiber::pause(),
    }
}

pub fn wait() -> Stub {
    Stub {
        hook: Hook::Await,
        code: schedule::wait(),
    }
}

pub fn all() -> Stub {
    Stub {
        hook: Hook::All,
        code: batch::all(),
    }
}

pub fn race() -> Stub {
    Stub {
        hook: Hook::Race,
        code: batch::race(),
    }
}

pub fn cancel() -> Stub {
    Stub {
        hook: Hook::Cancel,
        code: schedule::abort(),
    }
}

pub fn stub(hook: Hook) -> Stub {
    match hook {
        Hook::Start => start(),
        Hook::Exit => exit(),
        Hook::Write => write(),
        Hook::Spawn => spawn(),
        Hook::Yield => pause(),
        Hook::Await => wait(),
        Hook::All => all(),
        Hook::Race => race(),
        Hook::Cancel => cancel(),
    }
}

pub fn stubs() -> Vec<Stub> {
    Hook::ALL.into_iter().map(stub).collect()
}
