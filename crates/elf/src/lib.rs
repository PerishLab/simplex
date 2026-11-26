mod dynamic;
mod format;
mod image;
mod layout;
mod section;

use std::fmt;

pub const INTERPRETER: &str = "/lib64/ld-linux-x86-64.so.2";
pub const BASE: u64 = 0x0040_0000;
pub const PAGE: u64 = 0x1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Standalone,
    Dynamic,
    Shared,
}

impl Mode {
    pub const fn select(shared: bool, dynamic: bool) -> Self {
        match (shared, dynamic) {
            (true, _) => Self::Shared,
            (false, true) => Self::Dynamic,
            (false, false) => Self::Standalone,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Relative32,
    Absolute64,
    Procedure32,
    Global32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relocation {
    pub offset: u64,
    pub symbol: String,
    pub kind: Kind,
    pub addend: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub code: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub mode: Mode,
    pub entry: u64,
    pub needed: Vec<String>,
    pub code: Vec<u8>,
    pub data: Vec<u8>,
    pub exports: Vec<Symbol>,
    pub imports: Vec<String>,
    pub relocations: Vec<Relocation>,
}

impl Plan {
    pub fn standalone(code: Vec<u8>, data: Vec<u8>) -> Self {
        Self {
            mode: Mode::Standalone,
            entry: 0,
            needed: Vec::new(),
            code,
            data,
            exports: Vec::new(),
            imports: Vec::new(),
            relocations: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Empty,
    Unresolved(String),
    Unsupported(Mode),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => out.write_str("empty machine code section"),
            Self::Unresolved(symbol) => write!(out, "unresolved symbol: {symbol}"),
            Self::Unsupported(mode) => write!(out, "unimplemented elf mode: {mode:?}"),
        }
    }
}

impl std::error::Error for Fault {}

pub fn write(plan: &Plan) -> Result<Vec<u8>, Fault> {
    if plan.code.is_empty() && plan.mode != Mode::Shared {
        return Err(Fault::Empty);
    }
    if plan.mode == Mode::Shared {
        return Err(Fault::Unsupported(plan.mode));
    }
    image::Writer::new(plan)?.emit()
}
