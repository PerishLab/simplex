use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Elf,
    Portable,
    Mach,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Standalone,
    Dynamic,
    Shared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixup {
    Call,
    Relative,
    Got,
    Page,
    Low { scale: u8 },
    Global,
    Pointer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relocation {
    pub offset: u64,
    pub symbol: String,
    pub kind: Fixup,
    pub addend: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub code: bool,
    pub exported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub name: String,
    pub library: String,
    pub code: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub format: Format,
    pub mode: Mode,
    pub name: String,
    pub entry: u64,
    pub code: Vec<u8>,
    pub data: Vec<u8>,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub relocations: Vec<Relocation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Empty,
    Unresolved(String),
    Invalid(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => out.write_str("empty machine code section"),
            Self::Unresolved(symbol) => write!(out, "unresolved symbol: {symbol}"),
            Self::Invalid(message) => write!(out, "invalid object: {message}"),
        }
    }
}

impl std::error::Error for Fault {}

pub fn emit(artifact: &Artifact) -> Result<Vec<u8>, Fault> {
    if artifact.code.is_empty() {
        return Err(Fault::Empty);
    }
    Err(Fault::Invalid("unimplemented".to_string()))
}
