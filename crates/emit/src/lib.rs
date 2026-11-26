pub mod encode;
pub mod lower;
pub mod machine;
pub mod plan;

use simplex_elf::Plan;
use simplex_hir::Unit;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Register {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl Register {
    pub const ARGUMENTS: [Self; 6] = [
        Self::Rdi,
        Self::Rsi,
        Self::Rdx,
        Self::Rcx,
        Self::R8,
        Self::R9,
    ];
    pub const RESULTS: [Self; 2] = [Self::Rax, Self::Rdx];
    pub const PRESERVED: [Self; 5] = [Self::Rbx, Self::R12, Self::R13, Self::R14, Self::R15];

    pub const fn code(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    Immediate {
        dst: u32,
        value: i64,
    },
    Move {
        dst: u32,
        src: u32,
    },
    Add {
        dst: u32,
        lhs: u32,
        rhs: u32,
    },
    Sub {
        dst: u32,
        lhs: u32,
        rhs: u32,
    },
    Call {
        dst: Option<u32>,
        symbol: String,
        args: Vec<u32>,
    },
    Return {
        value: Option<u32>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Entry,
    Unimplemented(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entry => out.write_str("executable unit defines no main entry"),
            Self::Unimplemented(detail) => write!(out, "unimplemented codegen: {detail}"),
        }
    }
}

impl std::error::Error for Fault {}

pub fn compile(unit: &Unit) -> Result<Plan, Fault> {
    if !unit.shared && unit.entry.is_none() {
        return Err(Fault::Entry);
    }
    lower::build(unit)
}
