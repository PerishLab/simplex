use crate::bridge::ALLOC;
use crate::bridge::builder::Builder;
use crate::bridge::types::{BYTE, WIDE, infer};
use crate::tree::Expression;
use simplex_ir::{Binary, Operation};

impl Builder<'_, '_> {
    pub(super) fn window(
        &mut self,
        base: &Expression,
        bounds: (&Expression, &Expression),
        kind: &crate::Type,
    ) -> u32 {
        let (data, _) = self.sequence(base);
        let start = self.wide(bounds.0);
        let limit = self.wide(bounds.1);
        let elem = match kind {
            crate::Type::Slice(inner) | crate::Type::List(inner) => (**inner).clone(),
            _ => crate::Type::Unsigned(1),
        };
        let stride = elem.footprint(&self.unit.catalog).size.max(1) as i128;
        let scale = self.integer(stride, WIDE);
        let delta = self.emit(
            WIDE,
            Operation::Binary {
                op: Binary::Mul,
                left: start,
                right: scale,
                kind: WIDE,
            },
        );
        let low = self.advance(data, delta);
        let high = self.emit(
            WIDE,
            Operation::Binary {
                op: Binary::Sub,
                left: limit,
                right: start,
                kind: WIDE,
            },
        );
        self.emit(simplex_ir::Type::Pair, Operation::Pair { low, high })
    }

    pub(super) fn record(
        &mut self,
        name: &str,
        fields: &[(String, Expression)],
        kind: &crate::Type,
    ) -> u32 {
        let footprint = kind.footprint(&self.unit.catalog);
        if footprint.size == 0 {
            for (_, expr) in fields {
                let _ = self.expression(expr);
            }
            return 0;
        }
        let base = self.slot(footprint.size, footprint.align);
        self.clear(base, footprint.size);
        let records = &self.unit.catalog.records;
        let schema = records
            .get(name)
            .map(|item| item.fields.clone())
            .unwrap_or_default();
        for (label, expr) in fields {
            let actual = self.classify(expr);
            let raw = self.expression(expr);
            if let Some(slot) = schema.iter().find(|item| &item.name == label) {
                let value = self.recast(raw, &actual, &slot.kind);
                let address = self.displace(base, slot.offset as i64);
                self.store(address, value, &slot.kind);
            }
        }
        self.load(base, kind)
    }

    pub(super) fn variant(
        &mut self,
        choice: (&str, &str, u64),
        payloads: &[Expression],
        kind: &crate::Type,
    ) -> u32 {
        let (cname, aname, tag) = choice;
        let footprint = kind.footprint(&self.unit.catalog);
        if footprint.size == 0 {
            return 0;
        }
        let base = self.slot(footprint.size, footprint.align);
        self.clear(base, footprint.size);
        let mark = self.integer(tag as i128, BYTE);
        let _ = self.emit(
            simplex_ir::Type::Void,
            Operation::Store {
                address: base,
                value: mark,
                kind: BYTE,
                volatile: false,
            },
        );
        let choices = &self.unit.catalog.choices;
        let schema = choices
            .get(cname)
            .and_then(|item| {
                item.arms
                    .iter()
                    .find(|arm| arm.name == aname || arm.tag == tag)
            })
            .map(|arm| arm.payloads.clone())
            .unwrap_or_default();
        for (index, expr) in payloads.iter().enumerate() {
            let actual = self.classify(expr);
            let raw = self.expression(expr);
            let field = schema
                .get(index)
                .map_or_else(|| infer(expr), |item| item.kind.clone());
            let value = self.recast(raw, &actual, &field);
            let offset = schema
                .get(index)
                .map_or(8 + (index * 8) as i64, |item| item.offset as i64);
            let address = self.displace(base, offset);
            self.store(address, value, &field);
        }
        self.load(base, kind)
    }

    pub(super) fn list(&mut self, items: &[Expression], element: &crate::Type) -> u32 {
        let stride = element.footprint(&self.unit.catalog).size.max(1);
        let bytes = (items.len() as u64 * stride).max(8);
        let size = self.integer(bytes as i128, WIDE);
        let low = self.allocate(size);
        for (index, item) in items.iter().enumerate() {
            let actual = self.classify(item);
            let raw = self.expression(item);
            let value = self.recast(raw, &actual, element);
            let address = self.displace(low, (index as u64 * stride) as i64);
            self.store(address, value, element);
        }
        let high = self.integer(items.len() as i128, WIDE);
        self.emit(simplex_ir::Type::Pair, Operation::Pair { low, high })
    }

    pub(super) fn buffer(&mut self, count: &Expression, element: &crate::Type) -> u32 {
        let high = self.wide(count);
        let stride = element.footprint(&self.unit.catalog).size.max(1) as i128;
        let scale = self.integer(stride, WIDE);
        let raw = self.emit(
            WIDE,
            Operation::Binary {
                op: Binary::Mul,
                left: high,
                right: scale,
                kind: WIDE,
            },
        );
        let eight = self.integer(8, WIDE);
        let total = self.emit(
            WIDE,
            Operation::Binary {
                op: Binary::Add,
                left: raw,
                right: eight,
                kind: WIDE,
            },
        );
        let low = self.allocate(total);
        self.emit(simplex_ir::Type::Pair, Operation::Pair { low, high })
    }

    pub fn allocate(&mut self, bytes: u32) -> u32 {
        self.vault.allocated = true;
        self.emit(
            simplex_ir::Type::Pointer,
            Operation::Call {
                symbol: String::from(ALLOC),
                arguments: vec![bytes],
                inputs: vec![WIDE],
                kind: simplex_ir::Type::Pointer,
            },
        )
    }

    pub(super) fn coerce(&mut self, value: &Expression, table: &str) -> u32 {
        let low = self.expression(value);
        let high = self.emit(
            simplex_ir::Type::Pointer,
            Operation::Symbol {
                name: table.to_owned(),
            },
        );
        self.emit(simplex_ir::Type::Pair, Operation::Pair { low, high })
    }
}
