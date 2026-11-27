pub mod alu;
pub mod encode;
pub(crate) mod link;
mod lower;
pub mod media;

use crate::machine::Bank;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Linux,
    Windows,
}

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
    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn decode(code: u8) -> Self {
        match code & 15 {
            0 => Self::Rax,
            1 => Self::Rcx,
            2 => Self::Rdx,
            3 => Self::Rbx,
            4 => Self::Rsp,
            5 => Self::Rbp,
            6 => Self::Rsi,
            7 => Self::Rdi,
            8 => Self::R8,
            9 => Self::R9,
            10 => Self::R10,
            11 => Self::R11,
            12 => Self::R12,
            13 => Self::R13,
            14 => Self::R14,
            _ => Self::R15,
        }
    }

    pub const fn linux() -> [Self; 6] {
        [
            Self::Rdi,
            Self::Rsi,
            Self::Rdx,
            Self::Rcx,
            Self::R8,
            Self::R9,
        ]
    }

    pub const fn windows() -> [Self; 4] {
        [Self::Rcx, Self::Rdx, Self::R8, Self::R9]
    }

    pub fn bank(flavor: Flavor) -> Bank {
        match flavor {
            Flavor::Linux => Bank {
                general: vec![7, 6, 8, 9, 3, 12, 13, 14, 15],
                preserved: vec![3, 12, 13, 14, 15],
                float: (2..=15).collect(),
            },
            Flavor::Windows => Bank {
                general: vec![8, 9, 3, 7, 6, 12, 13, 14, 15],
                preserved: vec![3, 7, 6, 12, 13, 14, 15],
                float: (2..=5).collect(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Byte,
    Word,
    Double,
    Quad,
}

impl Width {
    pub const fn bytes(bytes: u32) -> Self {
        match bytes {
            1 => Self::Byte,
            2 => Self::Word,
            4 => Self::Double,
            _ => Self::Quad,
        }
    }

    pub const fn bits(bits: u8) -> Self {
        match bits {
            1 | 8 => Self::Byte,
            16 => Self::Word,
            32 => Self::Double,
            _ => Self::Quad,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Condition {
    Below = 0x2,
    Over = 0x3,
    Equal = 0x4,
    Distinct = 0x5,
    Under = 0x6,
    Above = 0x7,
    Sign = 0x8,
    Positive = 0x9,
    Parity = 0xA,
    Ordered = 0xB,
    Less = 0xC,
    Least = 0xD,
    Most = 0xE,
    Greater = 0xF,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Shift {
    Left = 4,
    Logical = 5,
    Arithmetic = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    Single,
    Double,
}

impl Precision {
    pub const fn bits(bits: u8) -> Self {
        match bits {
            32 => Self::Single,
            _ => Self::Double,
        }
    }

    pub const fn prefix(self) -> u8 {
        match self {
            Self::Single => 0xF3,
            Self::Double => 0xF2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Label(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address {
    pub base: Register,
    pub offset: i32,
}

impl Address {
    pub const fn new(base: Register, offset: i32) -> Self {
        Self { base, offset }
    }
}
