use super::Value;
use simplex_ir::{Binary, Cast, Type, Unary};

pub fn mask(val: i128, kind: &Type) -> u64 {
    match kind {
        Type::Integer { bits, .. } if *bits < 64 => (val as u64) & ((1u64 << *bits) - 1),
        _ => val as u64,
    }
}

pub fn unary(op: Unary, raw: u64, kind: &Type) -> Value {
    let res = match op {
        Unary::Neg => (raw as i64).wrapping_neg() as u64,
        Unary::Not if matches!(kind, Type::Integer { bits: 1, .. }) => raw ^ 1,
        Unary::Not => !raw,
    };
    Value::Word(mask(i128::from(res), kind))
}

pub fn binary(op: Binary, ops: (u64, u64), kind: &Type) -> Value {
    let signed = matches!(kind, Type::Integer { signed: true, .. });
    let (a, b) = ops;
    let sa = extend(a, kind);
    let sb = extend(b, kind);
    let res = match op {
        Binary::Add => a.wrapping_add(b),
        Binary::Sub => a.wrapping_sub(b),
        Binary::Mul => a.wrapping_mul(b),
        Binary::Div if signed => (sa / sb) as u64,
        Binary::Div => a / b,
        Binary::Rem if signed => (sa % sb) as u64,
        Binary::Rem => a % b,
        Binary::And => a & b,
        Binary::Or => a | b,
        Binary::Xor => a ^ b,
        Binary::Shl => a.wrapping_shl(b as u32),
        Binary::Shr if signed => (sa.wrapping_shr(b as u32)) as u64,
        Binary::Shr => a.wrapping_shr(b as u32),
        Binary::Eq => u64::from(a == b),
        Binary::Ne => u64::from(a != b),
        Binary::Lt if signed => u64::from(sa < sb),
        Binary::Lt => u64::from(a < b),
        Binary::Le if signed => u64::from(sa <= sb),
        Binary::Le => u64::from(a <= b),
        Binary::Gt if signed => u64::from(sa > sb),
        Binary::Gt => u64::from(a > b),
        Binary::Ge if signed => u64::from(sa >= sb),
        Binary::Ge => u64::from(a >= b),
    };
    if matches!(
        op,
        Binary::Eq | Binary::Ne | Binary::Lt | Binary::Le | Binary::Gt | Binary::Ge
    ) {
        return Value::Word(res);
    }
    Value::Word(mask(i128::from(res), kind))
}

pub fn cast(op: Cast, raw: u64, from: &Type, to: &Type) -> Value {
    let val = match op {
        Cast::Widen if matches!(from, Type::Integer { signed: true, .. }) => {
            extend(raw, from) as u64
        }
        _ => raw,
    };
    Value::Word(mask(i128::from(val), to))
}

fn extend(raw: u64, kind: &Type) -> i64 {
    match kind {
        Type::Integer { bits, signed: true } if *bits < 64 => {
            let shift = 64 - *bits;
            ((raw << shift) as i64) >> shift
        }
        _ => raw as i64,
    }
}
