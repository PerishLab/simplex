use crate::layout::{Catalog, Footprint};
use crate::tree::Expression;

pub const BOOL: simplex_ir::Type = simplex_ir::Type::Integer {
    bits: 1,
    signed: false,
};

pub const BYTE: simplex_ir::Type = simplex_ir::Type::Integer {
    bits: 8,
    signed: false,
};

pub const HALF: simplex_ir::Type = simplex_ir::Type::Integer {
    bits: 16,
    signed: false,
};

pub const WORD: simplex_ir::Type = simplex_ir::Type::Integer {
    bits: 32,
    signed: false,
};

pub const WIDE: simplex_ir::Type = simplex_ir::Type::Integer {
    bits: 64,
    signed: false,
};

pub fn convert(kind: &crate::Type, catalog: &Catalog) -> simplex_ir::Type {
    match kind {
        crate::Type::Void => simplex_ir::Type::Void,
        crate::Type::Bool => BOOL,
        crate::Type::Signed(width) => simplex_ir::Type::Integer {
            bits: width.saturating_mul(8).max(8),
            signed: true,
        },
        crate::Type::Unsigned(width) => simplex_ir::Type::Integer {
            bits: width.saturating_mul(8).max(8),
            signed: false,
        },
        crate::Type::Float(4) => simplex_ir::Type::Float { bits: 32 },
        crate::Type::Float(_) => simplex_ir::Type::Float { bits: 64 },
        crate::Type::Pointer(_) | crate::Type::Fiber(_) | crate::Type::Function { .. } => {
            simplex_ir::Type::Pointer
        }
        crate::Type::Slice(_) | crate::Type::Trait(_) | crate::Type::List(_) => {
            simplex_ir::Type::Pair
        }
        crate::Type::Record(name) => catalog
            .records
            .get(name)
            .map_or(simplex_ir::Type::Void, |item| aggregate(item.footprint)),
        crate::Type::Choice(name) => catalog
            .choices
            .get(name)
            .map_or(simplex_ir::Type::Void, |item| aggregate(item.footprint)),
    }
}

fn aggregate(footprint: Footprint) -> simplex_ir::Type {
    match footprint.size {
        0 => simplex_ir::Type::Void,
        size => simplex_ir::Type::Aggregate {
            size: size as u32,
            align: footprint.align.max(1) as u32,
            packed: false,
        },
    }
}

pub fn infer(expr: &Expression) -> crate::Type {
    match expr {
        Expression::Void | Expression::Yield => crate::Type::Void,
        Expression::Bool(_) => crate::Type::Bool,
        Expression::Bytes(_) => crate::Type::Slice(Box::new(crate::Type::Unsigned(1))),
        Expression::Length(_) => crate::Type::Unsigned(8),
        Expression::Int(_, kind)
        | Expression::Float(_, kind)
        | Expression::Symbol(_, kind)
        | Expression::Local(_, kind)
        | Expression::Unary { kind, .. }
        | Expression::Binary { kind, .. }
        | Expression::Cast { kind, .. }
        | Expression::Borrow { kind, .. }
        | Expression::Load { kind, .. }
        | Expression::Field { kind, .. }
        | Expression::Index { kind, .. }
        | Expression::Window { kind, .. }
        | Expression::Record { kind, .. }
        | Expression::Variant { kind, .. }
        | Expression::List { kind, .. }
        | Expression::Buffer { kind, .. }
        | Expression::Coerce { kind, .. }
        | Expression::Call { kind, .. }
        | Expression::Virtual { kind, .. }
        | Expression::Indirect { kind, .. }
        | Expression::Block { kind, .. }
        | Expression::Branch { kind, .. }
        | Expression::Switch { kind, .. }
        | Expression::Loop { kind, .. }
        | Expression::Await { kind, .. }
        | Expression::All { kind, .. }
        | Expression::Race { kind, .. } => kind.clone(),
    }
}
