use crate::layout::Type;
use crate::tree::Expression;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Void,
    Int(i128, Type),
    Bool(bool),
    Tag {
        choice: String,
        arm: String,
        tag: u64,
    },
    Count(usize),
    Field(Vec<String>),
}

impl Value {
    pub fn into(self) -> (Expression, Type) {
        match self {
            Self::Int(n, kind) => (Expression::Int(n, kind.clone()), kind),
            Self::Bool(b) => (Expression::Bool(b), Type::Bool),
            Self::Tag { choice, arm, tag } => {
                let kind = Type::Choice(choice.clone());
                let expr = Expression::Variant {
                    choice,
                    arm,
                    tag,
                    payloads: Vec::new(),
                    kind: kind.clone(),
                };
                (expr, kind)
            }
            Self::Count(n) => {
                let kind = Type::Unsigned(8);
                (Expression::Int(n as i128, kind.clone()), kind)
            }
            Self::Void | Self::Field(_) => (Expression::Void, Type::Void),
        }
    }
}

pub fn primitive(name: &str) -> Option<Type> {
    match name {
        "bool" => Some(Type::Bool),
        "i8" => Some(Type::Signed(1)),
        "i16" => Some(Type::Signed(2)),
        "i32" => Some(Type::Signed(4)),
        "i64" | "isize" => Some(Type::Signed(8)),
        "u8" => Some(Type::Unsigned(1)),
        "u16" => Some(Type::Unsigned(2)),
        "u32" => Some(Type::Unsigned(4)),
        "u64" | "usize" => Some(Type::Unsigned(8)),
        "f32" => Some(Type::Float(4)),
        "f64" => Some(Type::Float(8)),
        "str" | "Text" | "Bytes" => Some(Type::Slice(Box::new(Type::Unsigned(1)))),
        _ => None,
    }
}

pub fn encode(kind: &Type) -> String {
    match kind {
        Type::Void => String::from("void"),
        Type::Bool => String::from("bool"),
        Type::Signed(n) => format!("i{}", n * 8),
        Type::Unsigned(n) => format!("u{}", n * 8),
        Type::Float(n) => format!("f{}", n * 8),
        Type::Pointer(inner) => format!("ptr${}", encode(inner)),
        Type::Slice(inner) => format!("slice${}", encode(inner)),
        Type::List(inner) => format!("list${}", encode(inner)),
        Type::Fiber(inner) => format!("fiber${}", encode(inner)),
        Type::Trait(name) | Type::Record(name) | Type::Choice(name) => name.clone(),
        Type::Function { .. } => String::from("fn"),
    }
}

pub fn digits(text: &str) -> i128 {
    let clean = text.replace('_', "");
    if let Some(hex) = clean.strip_prefix("0x") {
        u128::from_str_radix(hex, 16).unwrap_or(0) as i128
    } else if let Some(bin) = clean.strip_prefix("0b") {
        u128::from_str_radix(bin, 2).unwrap_or(0) as i128
    } else if let Some(oct) = clean.strip_prefix("0o") {
        u128::from_str_radix(oct, 8).unwrap_or(0) as i128
    } else {
        clean
            .parse::<i128>()
            .or_else(|_| clean.parse::<u128>().map(|n| n as i128))
            .unwrap_or(0)
    }
}

pub fn float(text: &str, kind: &Type) -> u64 {
    let clean = text.replace('_', "");
    match kind {
        Type::Float(4) => clean.parse::<f32>().unwrap_or(0.0).to_bits() as u64,
        _ => clean.parse::<f64>().unwrap_or(0.0).to_bits(),
    }
}

pub fn retype(val: Value, kind: Type) -> Value {
    match val {
        Value::Int(n, _) => Value::Int(narrow(n, &kind), kind),
        other => other,
    }
}

pub fn narrow(n: i128, kind: &Type) -> i128 {
    match kind {
        Type::Unsigned(1) => (n as u8) as i128,
        Type::Unsigned(2) => (n as u16) as i128,
        Type::Unsigned(4) => (n as u32) as i128,
        Type::Unsigned(8) => (n as u64) as i128,
        Type::Signed(1) => (n as i8) as i128,
        Type::Signed(2) => (n as i16) as i128,
        Type::Signed(4) => (n as i32) as i128,
        _ => (n as i64) as i128,
    }
}

pub fn binary(op: &str, left: Value, right: Value) -> Value {
    match (op, left, right) {
        ("==", Value::Int(a, _), Value::Int(b, _)) => Value::Bool(a == b),
        ("!=", Value::Int(a, _), Value::Int(b, _)) => Value::Bool(a != b),
        ("==", a, b) => Value::Bool(a == b),
        ("!=", a, b) => Value::Bool(a != b),
        ("&&", Value::Bool(a), Value::Bool(b)) => Value::Bool(a && b),
        ("||", Value::Bool(a), Value::Bool(b)) => Value::Bool(a || b),
        (_, Value::Int(a, k), Value::Int(b, _)) => numeric(op, a, b, k),
        _ => Value::Void,
    }
}

pub fn numeric(op: &str, a: i128, b: i128, kind: Type) -> Value {
    match op {
        "+" => Value::Int(a.wrapping_add(b), kind),
        "-" => Value::Int(a.wrapping_sub(b), kind),
        "*" => Value::Int(a.wrapping_mul(b), kind),
        "/" if b != 0 => Value::Int(a / b, kind),
        "%" if b != 0 => Value::Int(a % b, kind),
        "&" => Value::Int(a & b, kind),
        "|" => Value::Int(a | b, kind),
        "^" => Value::Int(a ^ b, kind),
        "<<" => Value::Int(a << (b as u32), kind),
        ">>" => Value::Int(a >> (b as u32), kind),
        "<" => Value::Bool(a < b),
        "<=" => Value::Bool(a <= b),
        ">" => Value::Bool(a > b),
        ">=" => Value::Bool(a >= b),
        _ => Value::Void,
    }
}

pub fn mutate(op: &str, current: Option<Value>, rhs: Value) -> Value {
    if op == "=" {
        return rhs;
    }
    let (Some(Value::Int(a, k)), Value::Int(b, _)) = (current, rhs) else {
        return Value::Void;
    };
    numeric(op.trim_end_matches('='), a, b, k)
}
