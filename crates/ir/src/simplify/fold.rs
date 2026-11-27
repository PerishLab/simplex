use crate::{Binary, Operation, Type, Unary};

pub fn evaluate(
    constants: &[Option<(i128, Type)>],
    op: &Operation,
    out: &Type,
) -> Option<(i128, Type)> {
    match op {
        Operation::Integer { value, kind } => {
            let Type::Integer { bits, signed } = *kind else {
                return None;
            };
            Some((normalize(*value, bits, signed), kind.clone()))
        }
        Operation::Unary {
            op: kind,
            value,
            kind: domain,
        } => {
            let (num, _) = constants.get(*value as usize)?.clone()?;
            let Type::Integer { bits, signed } = *domain else {
                return None;
            };
            let raw = match kind {
                Unary::Neg => num.wrapping_neg(),
                Unary::Not if bits == 1 => num ^ 1,
                Unary::Not => !num,
            };
            Some((normalize(raw, bits, signed), out.clone()))
        }
        Operation::Binary {
            op: kind,
            left,
            right,
            kind: domain,
        } => {
            let (lhs, _) = constants.get(*left as usize)?.clone()?;
            let (rhs, _) = constants.get(*right as usize)?.clone()?;
            let Type::Integer { bits, signed } = *domain else {
                return None;
            };
            let val = compute(*kind, (lhs, rhs), (bits, signed))?;
            let Type::Integer {
                bits: width,
                signed: flag,
            } = *out
            else {
                return None;
            };
            Some((normalize(val, width, flag), out.clone()))
        }
        _ => None,
    }
}

pub fn normalize(value: i128, bits: u8, signed: bool) -> i128 {
    if bits >= 128 {
        return value;
    }
    let mask = (1u128 << bits) - 1;
    let raw = (value as u128) & mask;
    match signed {
        false => raw as i128,
        true => {
            let shift = 128 - bits;
            ((raw << shift) as i128) >> shift
        }
    }
}

fn compute(op: Binary, pair: (i128, i128), spec: (u8, bool)) -> Option<i128> {
    let (bits, signed) = spec;
    let lhs = normalize(pair.0, bits, signed);
    let rhs = normalize(pair.1, bits, signed);
    let mask = match bits >= 128 {
        true => u128::MAX,
        false => (1u128 << bits) - 1,
    };
    let ul = (lhs as u128) & mask;
    let ur = (rhs as u128) & mask;
    let amount = (ur as u32) & ((bits as u32).saturating_sub(1));
    let res = match op {
        Binary::Add => lhs.wrapping_add(rhs),
        Binary::Sub => lhs.wrapping_sub(rhs),
        Binary::Mul => lhs.wrapping_mul(rhs),
        Binary::Div if rhs == 0 => return None,
        Binary::Div if signed => lhs.wrapping_div(rhs),
        Binary::Div => (ul / ur) as i128,
        Binary::Rem if rhs == 0 => return None,
        Binary::Rem if signed => lhs.wrapping_rem(rhs),
        Binary::Rem => (ul % ur) as i128,
        Binary::And => (ul & ur) as i128,
        Binary::Or => (ul | ur) as i128,
        Binary::Xor => (ul ^ ur) as i128,
        Binary::Shl => ul.wrapping_shl(amount) as i128,
        Binary::Shr if signed => lhs.wrapping_shr(amount),
        Binary::Shr => ul.wrapping_shr(amount) as i128,
        other => compare(other, (lhs, rhs), (ul, ur), signed),
    };
    Some(res)
}

fn compare(op: Binary, signed: (i128, i128), unsigned: (u128, u128), flag: bool) -> i128 {
    let hit = match (op, flag) {
        (Binary::Eq, _) => unsigned.0 == unsigned.1,
        (Binary::Ne, _) => unsigned.0 != unsigned.1,
        (Binary::Lt, true) => signed.0 < signed.1,
        (Binary::Lt, false) => unsigned.0 < unsigned.1,
        (Binary::Le, true) => signed.0 <= signed.1,
        (Binary::Le, false) => unsigned.0 <= unsigned.1,
        (Binary::Gt, true) => signed.0 > signed.1,
        (Binary::Gt, false) => unsigned.0 > unsigned.1,
        (Binary::Ge, true) => signed.0 >= signed.1,
        (Binary::Ge, false) => unsigned.0 >= unsigned.1,
        _ => false,
    };
    i128::from(hit)
}
