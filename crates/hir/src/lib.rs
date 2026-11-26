use simplex::Finding;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Passing {
    Void,
    Register,
    Pair,
    Indirect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Footprint {
    pub size: u64,
    pub align: u64,
    pub passing: Passing,
}

impl Footprint {
    pub const VOID: Self = Self {
        size: 0,
        align: 1,
        passing: Passing::Void,
    };

    pub const fn scalar(size: u64) -> Self {
        Self {
            size,
            align: size,
            passing: Passing::Register,
        }
    }

    pub const fn pair() -> Self {
        Self {
            size: 16,
            align: 8,
            passing: Passing::Pair,
        }
    }

    pub const fn aggregate(size: u64, align: u64) -> Self {
        let passing = match size {
            0 => Passing::Void,
            1..=8 => Passing::Register,
            9..=16 => Passing::Pair,
            _ => Passing::Indirect,
        };
        Self {
            size,
            align,
            passing,
        }
    }
}

pub struct Mangle;

impl Mangle {
    pub fn symbol(ns: &str, module: &str, name: &str) -> String {
        format!("sx${ns}${module}${name}")
    }

    pub fn table(ns: &str, module: &str, subject: &str, contract: &str) -> String {
        format!("sx$vt${ns}${module}${subject}${contract}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Void,
    Bool,
    Signed(u8),
    Unsigned(u8),
    Float(u8),
    Pointer(Box<Type>),
    Slice(Box<Type>),
    Trait(String),
    Record(String),
    Choice(String),
    Fiber(Box<Type>),
}

impl Type {
    pub fn footprint(&self, catalog: &Catalog) -> Footprint {
        match self {
            Self::Void => Footprint::VOID,
            Self::Bool | Self::Signed(1) | Self::Unsigned(1) => Footprint::scalar(1),
            Self::Signed(2) | Self::Unsigned(2) => Footprint::scalar(2),
            Self::Signed(4) | Self::Unsigned(4) | Self::Float(4) => Footprint::scalar(4),
            Self::Signed(_) | Self::Unsigned(_) | Self::Float(_) => Footprint::scalar(8),
            Self::Pointer(_) | Self::Fiber(_) => Footprint::scalar(8),
            Self::Slice(_) | Self::Trait(_) => Footprint::pair(),
            Self::Record(name) => catalog
                .records
                .get(name)
                .map_or(Footprint::VOID, |r| r.footprint),
            Self::Choice(name) => catalog
                .choices
                .get(name)
                .map_or(Footprint::VOID, |c| c.footprint),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub records: BTreeMap<String, Layout>,
    pub choices: BTreeMap<String, Tagged>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub offset: u64,
    pub kind: Type,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub name: String,
    pub footprint: Footprint,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arm {
    pub name: String,
    pub tag: u64,
    pub payloads: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tagged {
    pub name: String,
    pub discriminant: u8,
    pub footprint: Footprint,
    pub arms: Vec<Arm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub local: String,
    pub symbol: String,
    pub library: Option<String>,
    pub inputs: Vec<Type>,
    pub output: Type,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub symbol: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub symbol: String,
    pub subject: String,
    pub contract: String,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    Int(i128, Type),
    Bool(bool),
    Bytes(String),
    Local(String, Type),
    Binary {
        operator: String,
        left: Box<Expression>,
        right: Box<Expression>,
        kind: Type,
    },
    Call {
        symbol: String,
        arguments: Vec<Expression>,
        kind: Type,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    Bind {
        name: String,
        mutable: bool,
        kind: Type,
        value: Expression,
    },
    Assign {
        name: String,
        value: Expression,
    },
    Evaluate(Expression),
    Result(Expression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Routine {
    pub symbol: String,
    pub exported: bool,
    pub asynchronous: bool,
    pub inputs: Vec<(String, Type)>,
    pub output: Type,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub namespace: String,
    pub shared: bool,
    pub libraries: Vec<String>,
    pub imports: Vec<Import>,
    pub assets: Vec<Asset>,
    pub tables: Vec<Table>,
    pub catalog: Catalog,
    pub routines: Vec<Routine>,
    pub entry: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
    Io(String),
    Frontend(Finding),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path) => write!(out, "unreadable module: {path}"),
            Self::Frontend(finding) => write!(out, "{finding}"),
        }
    }
}

impl std::error::Error for Fault {}

pub fn lower(namespace: &str, entry: &Path, shared: bool) -> Result<Unit, Fault> {
    let source = fs::read_to_string(entry).map_err(|_| Fault::Io(entry.display().to_string()))?;
    let name = entry.file_name().and_then(|s| s.to_str());
    simplex::inspect(name, entry.parent(), &source).map_err(Fault::Frontend)?;
    let stem = entry.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
    let main = (!shared).then(|| Mangle::symbol(namespace, stem, "main"));
    Ok(Unit {
        namespace: namespace.to_owned(),
        shared,
        libraries: Vec::new(),
        imports: Vec::new(),
        assets: Vec::new(),
        tables: Vec::new(),
        catalog: Catalog::default(),
        routines: Vec::new(),
        entry: main,
    })
}
