mod branch;

use super::builder::{Builder, Loop};
use super::types::convert;
use crate::tree::{Expression, Statement};
use simplex_ir::{Operation, Target, Terminator};

impl Builder<'_, '_> {
    pub fn statement(&mut self, item: &Statement) {
        match item {
            Statement::Bind {
                name, kind, value, ..
            } => {
                let actual = self.classify(value);
                let raw = self.expression(value);
                if !self.sealed {
                    let evaluated = self.recast(raw, &actual, kind);
                    self.bind(name, kind, evaluated);
                }
            }
            Statement::Assign { name, value } => self.assign(name, value),
            Statement::Store { target, value } => {
                let kind = self.classify(target);
                let address = self.place(target);
                let actual = self.classify(value);
                let raw = self.expression(value);
                let evaluated = self.recast(raw, &actual, &kind);
                self.store(address, evaluated, &kind);
            }
            Statement::Drop {
                name,
                symbol,
                borrowed,
            } => self.release(name, symbol, *borrowed),
            Statement::Break(value) => self.escape(value.as_ref()),
            Statement::Continue => self.again(),
            Statement::Evaluate(expr) => {
                let _ = self.expression(expr);
            }
            Statement::Result(expr) => {
                let actual = self.classify(expr);
                let raw = self.expression(expr);
                let expected = self.output.clone();
                let value = self.recast(raw, &actual, &expected);
                self.complete(value);
            }
        }
    }

    fn assign(&mut self, name: &str, value: &Expression) {
        let actual = self.classify(value);
        let raw = self.expression(value);
        let Some(local) = self.lookup(name).cloned() else {
            return;
        };
        let evaluated = self.recast(raw, &actual, &local.kind);
        self.store(local.address, evaluated, &local.kind);
        if let Some(bit) = local.bit {
            self.toggle(bit, true);
        }
    }

    pub fn scoped(&mut self, body: &[Statement], tail: Option<&Expression>) -> u32 {
        self.scopes.push(Vec::new());
        self.statements(body, false);
        let value = match (!self.sealed, tail) {
            (true, Some(expr)) => self.expression(expr),
            _ => 0,
        };
        self.scopes.pop();
        value
    }

    pub fn again(&mut self) {
        if let Some(top) = self.loops.last() {
            let head = top.head;
            self.terminate(Terminator::Jump(Target {
                block: head,
                arguments: Vec::new(),
            }));
        }
    }

    pub fn escape(&mut self, value: Option<&Expression>) {
        let pair = value.map(|expr| {
            let actual = self.classify(expr);
            (self.expression(expr), actual)
        });
        let Some(top) = self.loops.last().cloned() else {
            return;
        };
        let lowered = convert(&top.kind, &self.unit.catalog);
        let arguments = match lowered == simplex_ir::Type::Void {
            true => Vec::new(),
            false => {
                let chosen = match pair {
                    Some((raw, actual)) => self.recast(raw, &actual, &top.kind),
                    None => self.zero(&top.kind),
                };
                vec![chosen]
            }
        };
        if let Some(active) = self.loops.last_mut() {
            active.broke = true;
        }
        self.terminate(Terminator::Jump(Target {
            block: top.exit,
            arguments,
        }));
    }

    fn release(&mut self, name: &str, symbol: &str, borrowed: bool) {
        let Some(local) = self.lookup(name).cloned() else {
            return;
        };
        if let Some(bit) = local.bit {
            self.toggle(bit, false);
        }
        let (argument, input) = match (&local.kind, borrowed) {
            (crate::Type::Pointer(_), _) => (
                self.load(local.address, &local.kind),
                simplex_ir::Type::Pointer,
            ),
            (_, true) => (local.address, simplex_ir::Type::Pointer),
            (_, false) => {
                let loaded = self.load(local.address, &local.kind);
                (loaded, convert(&local.kind, &self.unit.catalog))
            }
        };
        let resolved = self.resolve(symbol);
        let expected = resolved
            .inputs
            .first()
            .map(|kind| convert(kind, &self.unit.catalog))
            .unwrap_or(input);
        let out = convert(&resolved.output, &self.unit.catalog);
        if resolved.missing {
            self.vault
                .unresolved
                .entry(resolved.symbol.clone())
                .or_insert_with(|| (vec![expected.clone()], out.clone()));
        }
        let _ = self.emit(
            out.clone(),
            Operation::Call {
                symbol: resolved.symbol,
                arguments: vec![argument],
                inputs: vec![expected],
                kind: out,
            },
        );
    }

    pub fn repeat(&mut self, body: &[Statement], kind: &crate::Type) -> u32 {
        let lowered = convert(kind, &self.unit.catalog);
        let (parameters, result) = self.parameter(&lowered);
        let head = self.block(Vec::new());
        let exit = self.block(parameters);
        self.terminate(Terminator::Jump(Target {
            block: head,
            arguments: Vec::new(),
        }));
        self.activate(head, true);
        self.loops.push(Loop {
            head,
            exit,
            kind: kind.clone(),
            broke: false,
        });
        self.scopes.push(Vec::new());
        self.statements(body, false);
        self.scopes.pop();
        let broke = self.loops.pop().is_some_and(|item| item.broke);
        if !self.sealed {
            self.terminate(Terminator::Jump(Target {
                block: head,
                arguments: Vec::new(),
            }));
        }
        self.activate(exit, broke);
        result
    }

    pub(super) fn parameter(
        &mut self,
        lowered: &simplex_ir::Type,
    ) -> (Vec<(u32, simplex_ir::Type)>, u32) {
        match *lowered == simplex_ir::Type::Void {
            true => (Vec::new(), 0),
            false => {
                let value = self.fresh();
                (vec![(value, lowered.clone())], value)
            }
        }
    }

    pub(super) fn outgoing(&self, value: u32, kind: &crate::Type) -> Vec<u32> {
        match convert(kind, &self.unit.catalog) == simplex_ir::Type::Void {
            true => Vec::new(),
            false => vec![value],
        }
    }
}
