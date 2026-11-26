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

pub fn exit() -> Stub {
    Stub {
        hook: Hook::Exit,
        code: vec![0xB8, Syscall::Exit as u8, 0x00, 0x00, 0x00, 0x0F, 0x05],
    }
}

pub fn stubs() -> Vec<Stub> {
    vec![exit()]
}
