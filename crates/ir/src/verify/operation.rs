use super::{Context, Spot};
use crate::{Binary, Cast, Fault, Instruction, Operation, Type, Unary};

impl Context<'_> {
    pub fn instruction(&self, spot: Spot, inst: &Instruction) -> Result<(), Fault> {
        self.operation(spot, inst)
    }

    fn operation(&self, spot: Spot, inst: &Instruction) -> Result<(), Fault> {
        match &inst.op {
            Operation::Integer { kind, .. } => self.expect(
                matches!(kind, Type::Integer { .. }) && &inst.kind == kind,
                "invalid integer constant",
            ),
            Operation::Float { kind, .. } => self.expect(
                matches!(kind, Type::Float { .. }) && &inst.kind == kind,
                "invalid float constant",
            ),
            Operation::Symbol { name } => self.expect(
                !name.is_empty() && inst.kind == Type::Pointer,
                "invalid symbol address",
            ),
            Operation::Slot { align, .. } => self.expect(
                *align >= 1 && align.is_power_of_two() && inst.kind == Type::Pointer,
                "invalid stack slot",
            ),
            Operation::Unary { op, value, kind } => {
                self.unary(spot, (*op, *value, kind), &inst.kind)
            }
            Operation::Binary {
                op,
                left,
                right,
                kind,
            } => self.binary(spot, (*op, *left, *right), (kind, &inst.kind)),
            Operation::Cast {
                op,
                value,
                from,
                kind,
            } => self.cast(spot, (*op, *value), (from, kind, &inst.kind)),
            Operation::Pair { low, high } => self.pair(spot, (*low, *high), &inst.kind),
            Operation::Low { pair, kind } | Operation::High { pair, kind } => {
                self.projection(spot, *pair, (kind, &inst.kind))
            }
            Operation::Offset { base, .. } => {
                self.require(spot, *base, &Type::Pointer)?;
                self.expect(inst.kind == Type::Pointer, "offset result must be pointer")
            }
            Operation::Load { address, kind, .. } => {
                self.require(spot, *address, &Type::Pointer)?;
                self.expect(kind.concrete() && &inst.kind == kind, "invalid load type")
            }
            Operation::Store {
                address,
                value,
                kind,
                ..
            } => {
                self.require(spot, *address, &Type::Pointer)?;
                self.require(spot, *value, kind)?;
                self.expect(
                    kind.concrete() && inst.kind == Type::Void,
                    "invalid store type",
                )
            }
            Operation::Copy {
                destination,
                source,
                ..
            } => {
                self.require(spot, *destination, &Type::Pointer)?;
                self.require(spot, *source, &Type::Pointer)?;
                self.expect(inst.kind == Type::Void, "copy must return void")
            }
            other => self.extended(spot, other, &inst.kind),
        }
    }

    fn unary(&self, spot: Spot, spec: (Unary, u32, &Type), out: &Type) -> Result<(), Fault> {
        let (op, value, kind) = spec;
        self.require(spot, value, kind)?;
        let allowed = match op {
            Unary::Neg => matches!(kind, Type::Integer { .. } | Type::Float { .. }),
            Unary::Not => matches!(kind, Type::Integer { .. }),
        };
        self.expect(allowed && out == kind, "invalid unary operation")
    }

    fn binary(
        &self,
        spot: Spot,
        ops: (Binary, u32, u32),
        kinds: (&Type, &Type),
    ) -> Result<(), Fault> {
        let (op, left, right) = ops;
        let (domain, out) = kinds;
        self.require(spot, left, domain)?;
        match op {
            Binary::Shl | Binary::Shr => {
                let shift = self.lookup(spot, right)?;
                self.expect(
                    matches!(shift, Type::Integer { .. }),
                    "shift count must be integer",
                )?;
            }
            _ => self.require(spot, right, domain)?,
        }
        if op.comparison() {
            let boolean = Type::Integer {
                bits: 1,
                signed: false,
            };
            return self.expect(domain.word() && *out == boolean, "invalid comparison");
        }
        let allowed = match op {
            Binary::Add | Binary::Sub | Binary::Mul | Binary::Div | Binary::Rem => {
                matches!(domain, Type::Integer { .. } | Type::Float { .. })
            }
            _ => matches!(domain, Type::Integer { .. }),
        };
        self.expect(allowed && out == domain, "invalid binary operation")
    }

    fn cast(
        &self,
        spot: Spot,
        ops: (Cast, u32),
        kinds: (&Type, &Type, &Type),
    ) -> Result<(), Fault> {
        let (op, value) = ops;
        let (from, to, out) = kinds;
        self.require(spot, value, from)?;
        self.expect(
            from.concrete() && to.concrete() && out == to,
            "invalid cast types",
        )?;
        let legal = match (op, from, to) {
            (Cast::Widen, Type::Integer { bits: a, .. }, Type::Integer { bits: b, .. }) => b >= a,
            (Cast::Narrow, Type::Integer { bits: a, .. }, Type::Integer { bits: b, .. }) => b <= a,
            (Cast::Promote | Cast::Elevate, Type::Integer { .. }, Type::Float { .. }) => true,
            (Cast::Truncate | Cast::Demote, Type::Float { .. }, Type::Integer { .. }) => true,
            (Cast::Extend, Type::Float { bits: a }, Type::Float { bits: b }) => b >= a,
            (Cast::Shrink, Type::Float { bits: a }, Type::Float { bits: b }) => b <= a,
            (Cast::Bitcast, a, b) => a.word() && b.word() && a.bytes() == b.bytes(),
            _ => false,
        };
        self.expect(legal, "incompatible cast operator")
    }

    fn pair(&self, spot: Spot, items: (u32, u32), out: &Type) -> Result<(), Fault> {
        let low = self.lookup(spot, items.0)?;
        let high = self.lookup(spot, items.1)?;
        self.expect(
            low.word() && high.word() && *out == Type::Pair,
            "invalid pair construction",
        )
    }

    fn projection(&self, spot: Spot, src: u32, kinds: (&Type, &Type)) -> Result<(), Fault> {
        self.require(spot, src, &Type::Pair)?;
        self.expect(
            kinds.0.word() && kinds.1 == kinds.0,
            "invalid pair projection",
        )
    }

    pub(super) fn expect(&self, condition: bool, message: &str) -> Result<(), Fault> {
        match condition {
            true => Ok(()),
            false => Err(self.fail(message)),
        }
    }
}
