mod model;

pub use model::*;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub value: u32,
    pub kind: Type,
    pub op: Operation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub block: u32,
    pub arguments: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Jump(Target),
    Branch {
        condition: u32,
        truthy: Target,
        falsy: Target,
    },
    Switch {
        value: u32,
        cases: Vec<(u64, Target)>,
        default: Target,
    },
    Return(Option<u32>),
    Trap {
        code: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: u32,
    pub parameters: Vec<(u32, Type)>,
    pub body: Vec<Instruction>,
    pub exit: Terminator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub symbol: String,
    pub linkage: Linkage,
    pub asynchronous: bool,
    pub cleanup: Option<String>,
    pub inputs: Vec<(u32, Type)>,
    pub output: Type,
    pub values: u32,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    pub shared: bool,
    pub entry: Option<String>,
    pub assets: Vec<Asset>,
    pub tables: Vec<Table>,
    pub externals: Vec<External>,
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Invalid(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => write!(out, "invalid ir: {reason}"),
        }
    }
}

impl std::error::Error for Fault {}

pub fn verify(module: &Module) -> Result<(), Fault> {
    let _ = module;
    Ok(())
}

pub fn simplify(module: &mut Module) {
    let _ = module;
}

pub fn prepare(module: &mut Module) -> Result<(), Fault> {
    verify(module)?;
    simplify(module);
    verify(module)
}
