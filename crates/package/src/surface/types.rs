#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Passing {
    Void,
    Register,
    Pair,
    Indirect,
}

impl Passing {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::Register => "register",
            Self::Pair => "pair",
            Self::Indirect => "indirect",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Footprint {
    pub(crate) size: u64,
    pub(crate) align: u64,
    pub(crate) passing: Passing,
}

impl Footprint {
    pub(crate) const VOID: Self = Self {
        size: 0,
        align: 1,
        passing: Passing::Void,
    };

    pub(crate) const fn scalar(size: u64) -> Self {
        Self {
            size,
            align: size,
            passing: Passing::Register,
        }
    }

    pub(crate) const fn pair() -> Self {
        Self {
            size: 16,
            align: 8,
            passing: Passing::Pair,
        }
    }

    pub(crate) const fn aggregate(size: u64, align: u64) -> Self {
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

    pub(crate) fn render(self) -> String {
        format!("{}/{}/{}", self.size, self.align, self.passing.name())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Void,
    Bool,
    Signed(u8),
    Unsigned(u8),
    Float(u8),
    Pointer(Box<Kind>),
    Slice(Box<Kind>),
    Trait(String),
    Record(String, Vec<Kind>),
    Choice(String, Vec<Kind>),
    List(Box<Kind>),
    Fiber(Box<Kind>),
    Function(Vec<Kind>, Box<Kind>),
    Origin,
    Param(String),
    External(String, Vec<Kind>),
}

impl Kind {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Void => String::from("void"),
            Self::Bool => String::from("bool"),
            Self::Signed(bytes) => format!("i{}", bytes * 8),
            Self::Unsigned(bytes) => format!("u{}", bytes * 8),
            Self::Float(bytes) => format!("f{}", bytes * 8),
            Self::Pointer(inner) => format!("*{}", inner.label()),
            Self::Slice(inner) => format!("[{}]", inner.label()),
            Self::Trait(name) => format!("dyn:{name}"),
            Self::Record(name, args) => parameterized(name, args),
            Self::Choice(name, args) => parameterized(name, args),
            Self::List(inner) => format!("List<{}>", inner.label()),
            Self::Fiber(inner) => format!("Fiber<{}>", inner.label()),
            Self::Function(inputs, output) => {
                let parts: Vec<String> = inputs.iter().map(Self::label).collect();
                format!("fn({})->{}", parts.join(","), output.label())
            }
            Self::Origin => String::from("Origin"),
            Self::Param(name) => format!("param:{name}"),
            Self::External(name, args) => parameterized(name, args),
        }
    }
}

fn parameterized(name: &str, args: &[Kind]) -> String {
    if args.is_empty() {
        return name.to_owned();
    }
    let parts: Vec<String> = args.iter().map(Kind::label).collect();
    format!("{name}<{}>", parts.join(","))
}

pub(crate) fn primitive(name: &str) -> Option<Kind> {
    match name {
        "void" => Some(Kind::Void),
        "bool" => Some(Kind::Bool),
        "i8" => Some(Kind::Signed(1)),
        "i16" => Some(Kind::Signed(2)),
        "i32" => Some(Kind::Signed(4)),
        "i64" | "isize" => Some(Kind::Signed(8)),
        "u8" => Some(Kind::Unsigned(1)),
        "u16" => Some(Kind::Unsigned(2)),
        "u32" => Some(Kind::Unsigned(4)),
        "u64" | "usize" => Some(Kind::Unsigned(8)),
        "f32" => Some(Kind::Float(4)),
        "f64" => Some(Kind::Float(8)),
        "str" | "Text" | "Bytes" => Some(Kind::Slice(Box::new(Kind::Unsigned(1)))),
        _ => None,
    }
}

pub(crate) fn pad(offset: u64, align: u64) -> u64 {
    let step = align.max(1);
    (offset + step - 1) & !(step - 1)
}
