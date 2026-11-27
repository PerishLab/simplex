use crate::bridge::builder::Builder;
use crate::bridge::types::WIDE;
use crate::tree::Expression;
use simplex_ir::{Binary, Operation};

impl Builder<'_, '_> {
    pub fn place(&mut self, expr: &Expression) -> u32 {
        match expr {
            Expression::Local(name, _) => self
                .lookup(name)
                .map(|item| item.address)
                .unwrap_or_else(|| self.slot(8, 8)),
            Expression::Load { pointer, .. } => self.expression(pointer),
            Expression::Field { base, offset, .. } => {
                let root = self.origin(base);
                self.displace(root, *offset as i64)
            }
            Expression::Index { base, index, kind } => {
                let (data, _) = self.sequence(base);
                let position = self.wide(index);
                let footprint = kind.footprint(&self.unit.catalog);
                let scale = self.integer(footprint.size.max(1) as i128, WIDE);
                let delta = self.emit(
                    WIDE,
                    Operation::Binary {
                        op: Binary::Mul,
                        left: position,
                        right: scale,
                        kind: WIDE,
                    },
                );
                self.advance(data, delta)
            }
            other => {
                let kind = self.classify(other);
                let value = self.expression(other);
                self.materialize(value, &kind)
            }
        }
    }

    pub fn origin(&mut self, expr: &Expression) -> u32 {
        match self.classify(expr) {
            crate::Type::Pointer(_) => self.expression(expr),
            _ => self.place(expr),
        }
    }

    pub fn sequence(&mut self, expr: &Expression) -> (u32, u32) {
        let pair = match self.classify(expr) {
            crate::Type::Pointer(inner) => {
                let pointer = self.expression(expr);
                self.load(pointer, &inner)
            }
            _ => self.expression(expr),
        };
        let data = self.emit(
            simplex_ir::Type::Pointer,
            Operation::Low {
                pair,
                kind: simplex_ir::Type::Pointer,
            },
        );
        let count = self.emit(WIDE, Operation::High { pair, kind: WIDE });
        (data, count)
    }

    pub fn wide(&mut self, expr: &Expression) -> u32 {
        let source = self.classify(expr);
        let value = self.expression(expr);
        self.recast(value, &source, &crate::Type::Unsigned(8))
    }
}
