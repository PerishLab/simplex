use crate::bridge::builder::Builder;
use crate::bridge::types::{BYTE, WIDE, convert};
use crate::tree::{Case, Expression};
use simplex_ir::{Cast, Operation, Target, Terminator};

impl Builder<'_, '_> {
    pub fn conditional(
        &mut self,
        condition: &Expression,
        arms: (&Expression, &Expression),
        kind: &crate::Type,
    ) -> u32 {
        let flag = self.expression(condition);
        if self.sealed {
            return 0;
        }
        let lowered = convert(kind, &self.unit.catalog);
        let (parameters, result) = self.parameter(&lowered);
        let first = self.block(Vec::new());
        let second = self.block(Vec::new());
        let joined = self.block(parameters);
        self.terminate(Terminator::Branch {
            condition: flag,
            truthy: Target {
                block: first,
                arguments: Vec::new(),
            },
            falsy: Target {
                block: second,
                arguments: Vec::new(),
            },
        });
        let left = self.branch((first, joined), arms.0, kind);
        let right = self.branch((second, joined), arms.1, kind);
        self.activate(joined, left || right);
        result
    }

    fn branch(&mut self, route: (u32, u32), expr: &Expression, kind: &crate::Type) -> bool {
        self.activate(route.0, true);
        let actual = self.classify(expr);
        let raw = self.expression(expr);
        if self.sealed {
            return false;
        }
        let value = self.recast(raw, &actual, kind);
        let arguments = self.outgoing(value, kind);
        self.terminate(Terminator::Jump(Target {
            block: route.1,
            arguments,
        }));
        true
    }

    pub fn select(&mut self, subject: &Expression, cases: &[Case], kind: &crate::Type) -> u32 {
        let domain = self.classify(subject);
        let (discriminant, base) = self.scrutinee(subject, &domain);
        if self.sealed {
            return 0;
        }
        let lowered = convert(kind, &self.unit.catalog);
        let (parameters, result) = self.parameter(&lowered);
        let joined = self.block(parameters);
        if cases.is_empty() {
            let fallback = self.zero(kind);
            let arguments = self.outgoing(fallback, kind);
            self.terminate(Terminator::Jump(Target {
                block: joined,
                arguments,
            }));
            self.activate(joined, true);
            return result;
        }
        let blocks: Vec<u32> = cases.iter().map(|_| self.block(Vec::new())).collect();
        let (entries, default) = targets(cases, &blocks);
        self.terminate(Terminator::Switch {
            value: discriminant,
            cases: entries,
            default,
        });
        let mut reached = false;
        for (item, target) in cases.iter().zip(blocks) {
            if self.arm((target, joined, base), item, kind) {
                reached = true;
            }
        }
        self.activate(joined, reached);
        result
    }

    fn scrutinee(&mut self, subject: &Expression, domain: &crate::Type) -> (u32, Option<u32>) {
        match domain {
            crate::Type::Choice(_) => {
                let value = self.expression(subject);
                let base = self.materialize(value, domain);
                let byte = self.emit(
                    BYTE,
                    Operation::Load {
                        address: base,
                        kind: BYTE,
                        volatile: false,
                    },
                );
                let wide = self.emit(
                    WIDE,
                    Operation::Cast {
                        op: Cast::Widen,
                        value: byte,
                        from: BYTE,
                        kind: WIDE,
                    },
                );
                (wide, Some(base))
            }
            _ => (self.wide(subject), None),
        }
    }

    fn arm(&mut self, route: (u32, u32, Option<u32>), item: &Case, kind: &crate::Type) -> bool {
        self.activate(route.0, true);
        self.scopes.push(Vec::new());
        if let Some(root) = route.2 {
            for (name, field) in &item.bindings {
                let address = self.displace(root, field.offset as i64);
                let loaded = self.load(address, &field.kind);
                self.bind(name, &field.kind, loaded);
            }
        }
        let actual = self.classify(&item.body);
        let raw = self.expression(&item.body);
        self.scopes.pop();
        if self.sealed {
            return false;
        }
        let value = self.recast(raw, &actual, kind);
        let arguments = self.outgoing(value, kind);
        self.terminate(Terminator::Jump(Target {
            block: route.1,
            arguments,
        }));
        true
    }
}

fn targets(cases: &[Case], blocks: &[u32]) -> (Vec<(u64, Target)>, Target) {
    let mut entries = Vec::new();
    let last = blocks.last().copied().unwrap_or(0);
    let mut fallback = Target {
        block: last,
        arguments: Vec::new(),
    };
    for (index, (item, &block)) in cases.iter().zip(blocks).enumerate() {
        if index + 1 == cases.len() || item.tags.is_empty() {
            fallback = Target {
                block,
                arguments: Vec::new(),
            };
            break;
        }
        for &tag in &item.tags {
            entries.push((
                tag,
                Target {
                    block,
                    arguments: Vec::new(),
                },
            ));
        }
    }
    (entries, fallback)
}
