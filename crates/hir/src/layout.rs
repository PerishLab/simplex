use std::collections::BTreeMap;

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

    pub fn method(table: &str, name: &str) -> String {
        match table.strip_prefix("sx$vt$") {
            Some(rest) => format!("sx$impl${rest}${name}"),
            None => format!("sx$impl${table}${name}"),
        }
    }

    pub fn asset(ns: &str, module: &str, index: usize) -> String {
        format!("sx$ro${ns}${module}${index}")
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
    List(Box<Type>),
    Fiber(Box<Type>),
    Function {
        inputs: Vec<Type>,
        output: Box<Type>,
    },
}

impl Type {
    pub fn footprint(&self, catalog: &Catalog) -> Footprint {
        match self {
            Self::Void => Footprint::VOID,
            Self::Bool | Self::Signed(1) | Self::Unsigned(1) => Footprint::scalar(1),
            Self::Signed(2) | Self::Unsigned(2) => Footprint::scalar(2),
            Self::Signed(4) | Self::Unsigned(4) | Self::Float(4) => Footprint::scalar(4),
            Self::Signed(_) | Self::Unsigned(_) | Self::Float(_) => Footprint::scalar(8),
            Self::Pointer(_) | Self::Fiber(_) | Self::Function { .. } => Footprint::scalar(8),
            Self::Slice(_) | Self::Trait(_) | Self::List(_) => Footprint::pair(),
            Self::Record(name) => catalog
                .records
                .get(name)
                .map_or(Footprint::VOID, |item| item.footprint),
            Self::Choice(name) => catalog
                .choices
                .get(name)
                .map_or(Footprint::VOID, |item| item.footprint),
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
