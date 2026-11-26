use super::Context;
use crate::code::{Code, Finding};
use crate::model::Table;
use crate::model::borrow::root;
use crate::tree::{Body, Expression, Loop, Spec};
use std::collections::BTreeSet;

impl<'a> Context<'a> {
    pub(super) fn function(&self, expr: &Expression) -> Result<(), Finding> {
        let Expression::Function {
            asynchronous,
            inputs,
            output,
            body,
            ..
        } = expr
        else {
            return Ok(());
        };
        if output.as_ref().is_some_and(mentions) {
            let pos = inputs.first().map_or((1, 1), |p| (p.line, p.column));
            return Err(Finding {
                code: Code::Detached,
                line: pos.0,
                column: pos.1,
            });
        }
        let mut inner = Context {
            table: self.table,
            immediate: *asynchronous,
            nursery: self.nursery || *asynchronous,
            fibers: BTreeSet::new(),
            targets: inputs
                .iter()
                .filter(|p| matches!(p.annotation, Some(Spec::Borrow { mutable: true, .. })))
                .map(|p| p.name.clone())
                .collect(),
            tainted: BTreeSet::new(),
        };
        match body {
            Body::Block(block) => inner.items(&block.items),
            Body::Switch(d) => inner.repetition(&Loop::Switch(d.clone())),
            Body::Bucket(p) => inner.repetition(&Loop::Bucket(p.clone())),
            Body::Loop(k) => inner.repetition(k),
        }
    }

    pub(super) fn pending(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Word { text, .. } => self.fibers.contains(text),
            Expression::Group(inner) => self.pending(inner),
            Expression::List { items, .. } => items.iter().any(|i| self.pending(i)),
            Expression::Call { callee, .. } => {
                spawns(self.table, expr) || launches(self.table, callee)
            }
            _ => false,
        }
    }

    pub(super) fn dirty(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Word { text, .. } => self.tainted.contains(text),
            Expression::List { items, .. } => items.iter().any(|i| self.dirty(i)),
            Expression::Borrow { mutable, .. } => *mutable,
            Expression::Call {
                callee, arguments, ..
            } => {
                arguments.iter().any(|a| self.dirty(a))
                    || (mutates(self.table, callee)
                        && arguments
                            .iter()
                            .any(|a| root(a).is_some_and(|r| self.targets.contains(&r))))
            }
            _ => false,
        }
    }
}

pub(super) fn launches(table: &Table, callee: &Expression) -> bool {
    match callee {
        Expression::Word { text, .. } => {
            text == "spawn" || table.functions.get(text).is_some_and(|s| s.asynchronous)
        }
        Expression::Member { field, .. } => {
            table
                .methods
                .iter()
                .any(|((_, m), s)| m == field && s.asynchronous)
                || table
                    .interfaces
                    .values()
                    .flat_map(|c| &c.methods)
                    .any(|r| r.name == *field && r.asynchronous)
        }
        _ => false,
    }
}

pub(super) fn mutates(table: &Table, callee: &Expression) -> bool {
    let Expression::Word { text, .. } = callee else {
        return false;
    };
    table.functions.get(text).is_some_and(|sig| {
        sig.inputs
            .iter()
            .flatten()
            .any(|s| matches!(s, Spec::Borrow { mutable: true, .. }))
    })
}

pub(crate) fn mentions(spec: &Spec) -> bool {
    match spec {
        Spec::Named {
            name, arguments, ..
        } => name == "Fiber" || arguments.iter().any(mentions),
        Spec::Borrow { target, .. } | Spec::Slice(target) => mentions(target),
        Spec::Callable { inputs, output, .. } => {
            inputs.iter().any(mentions) || output.as_deref().is_some_and(mentions)
        }
    }
}

pub(crate) fn spawns(table: &Table, expr: &Expression) -> bool {
    match expr {
        Expression::List { items, .. } => items.iter().any(|i| spawns(table, i)),
        Expression::Call { callee, .. } => {
            matches!(callee.as_ref(), Expression::Member { field, .. } if matches!(field.as_str(), "all" | "race"))
                || launches(table, callee)
        }
        _ => false,
    }
}
