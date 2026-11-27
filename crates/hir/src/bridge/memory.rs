use super::builder::{Builder, Local};
use super::types::{BYTE, HALF, WIDE, WORD, convert};
use simplex_ir::{Binary, Cast, Operation};

impl Builder<'_, '_> {
    pub fn integer(&mut self, value: i128, kind: simplex_ir::Type) -> u32 {
        self.emit(kind.clone(), Operation::Integer { value, kind })
    }

    pub fn zero(&mut self, kind: &crate::Type) -> u32 {
        let lowered = convert(kind, &self.unit.catalog);
        match lowered {
            simplex_ir::Type::Void => 0,
            simplex_ir::Type::Integer { .. } => self.integer(0, lowered),
            simplex_ir::Type::Float { .. } => self.emit(
                lowered.clone(),
                Operation::Float {
                    bits: 0,
                    kind: lowered,
                },
            ),
            simplex_ir::Type::Pointer => self.null(),
            simplex_ir::Type::Pair => self.empty(kind),
            simplex_ir::Type::Aggregate { size, align, .. } => {
                let base = self.slot(u64::from(size), u64::from(align));
                self.clear(base, u64::from(size));
                self.emit(
                    lowered.clone(),
                    Operation::Load {
                        address: base,
                        kind: lowered,
                        volatile: false,
                    },
                )
            }
        }
    }

    fn null(&mut self) -> u32 {
        let raw = self.integer(0, WIDE);
        self.emit(
            simplex_ir::Type::Pointer,
            Operation::Cast {
                op: Cast::Bitcast,
                value: raw,
                from: WIDE,
                kind: simplex_ir::Type::Pointer,
            },
        )
    }

    fn empty(&mut self, kind: &crate::Type) -> u32 {
        let low = self.null();
        let high = match kind {
            crate::Type::Trait(_) => low,
            _ => self.integer(0, WIDE),
        };
        self.emit(simplex_ir::Type::Pair, Operation::Pair { low, high })
    }

    pub fn clear(&mut self, base: u32, size: u64) {
        let mut cursor = 0u64;
        while cursor + 8 <= size {
            self.fill(base, cursor, WIDE);
            cursor += 8;
        }
        if cursor + 4 <= size {
            self.fill(base, cursor, WORD);
            cursor += 4;
        }
        if cursor + 2 <= size {
            self.fill(base, cursor, HALF);
            cursor += 2;
        }
        if cursor < size {
            self.fill(base, cursor, BYTE);
        }
    }

    fn fill(&mut self, base: u32, offset: u64, kind: simplex_ir::Type) {
        let address = self.displace(base, offset as i64);
        let zero = self.integer(0, kind.clone());
        let _ = self.emit(
            simplex_ir::Type::Void,
            Operation::Store {
                address,
                value: zero,
                kind,
                volatile: false,
            },
        );
    }

    pub fn displace(&mut self, base: u32, bytes: i64) -> u32 {
        match bytes {
            0 => base,
            delta => self.emit(
                simplex_ir::Type::Pointer,
                Operation::Offset { base, bytes: delta },
            ),
        }
    }

    pub fn advance(&mut self, base: u32, delta: u32) -> u32 {
        let start = self.emit(
            WIDE,
            Operation::Cast {
                op: Cast::Bitcast,
                value: base,
                from: simplex_ir::Type::Pointer,
                kind: WIDE,
            },
        );
        let moved = self.emit(
            WIDE,
            Operation::Binary {
                op: Binary::Add,
                left: start,
                right: delta,
                kind: WIDE,
            },
        );
        self.emit(
            simplex_ir::Type::Pointer,
            Operation::Cast {
                op: Cast::Bitcast,
                value: moved,
                from: WIDE,
                kind: simplex_ir::Type::Pointer,
            },
        )
    }

    pub fn load(&mut self, address: u32, kind: &crate::Type) -> u32 {
        let lowered = convert(kind, &self.unit.catalog);
        match lowered {
            simplex_ir::Type::Void => 0,
            target => self.emit(
                target.clone(),
                Operation::Load {
                    address,
                    kind: target,
                    volatile: false,
                },
            ),
        }
    }

    pub fn store(&mut self, address: u32, value: u32, kind: &crate::Type) {
        let lowered = convert(kind, &self.unit.catalog);
        if lowered != simplex_ir::Type::Void {
            let _ = self.emit(
                simplex_ir::Type::Void,
                Operation::Store {
                    address,
                    value,
                    kind: lowered,
                    volatile: false,
                },
            );
        }
    }

    pub fn materialize(&mut self, value: u32, kind: &crate::Type) -> u32 {
        let footprint = kind.footprint(&self.unit.catalog);
        let base = self.slot(footprint.size.max(1), footprint.align.max(1));
        self.store(base, value, kind);
        base
    }

    pub fn bind(&mut self, name: &str, kind: &crate::Type, value: u32) {
        let rank = self.order;
        self.order += 1;
        let tracked = self
            .plan
            .as_ref()
            .and_then(|spec| spec.items.iter().find(|item| item.order == rank).cloned());
        if let (Some(item), Some(root)) = (tracked, self.environment) {
            let address = self.displace(root, i64::from(item.offset));
            self.store(address, value, kind);
            self.toggle(item.bit, true);
            self.declare(Local {
                name: name.to_owned(),
                kind: kind.clone(),
                address,
                bit: Some(item.bit),
            });
            return;
        }
        let footprint = kind.footprint(&self.unit.catalog);
        let address = self.slot(footprint.size.max(1), footprint.align.max(1));
        self.store(address, value, kind);
        self.declare(Local {
            name: name.to_owned(),
            kind: kind.clone(),
            address,
            bit: None,
        });
    }

    fn declare(&mut self, local: Local) {
        if let Some(top) = self.scopes.last_mut() {
            top.push(local);
        }
    }

    pub fn toggle(&mut self, bit: u32, active: bool) {
        let Some(root) = self.environment else {
            return;
        };
        let mask = self.emit(
            WIDE,
            Operation::Load {
                address: root,
                kind: WIDE,
                volatile: false,
            },
        );
        let (op, immediate) = match active {
            true => (Binary::Or, 1i128 << bit),
            false => (Binary::And, (!(1u64 << bit) as i64) as i128),
        };
        let flag = self.integer(immediate, WIDE);
        let updated = self.emit(
            WIDE,
            Operation::Binary {
                op,
                left: mask,
                right: flag,
                kind: WIDE,
            },
        );
        let _ = self.emit(
            simplex_ir::Type::Void,
            Operation::Store {
                address: root,
                value: updated,
                kind: WIDE,
                volatile: false,
            },
        );
    }
}
