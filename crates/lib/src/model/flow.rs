use super::Table;
use super::borrow::{contains, root, source};
use super::count::Counter;
use crate::code::{Code, Finding};
use crate::tree::{Binding, Block, Body, Expression, Item, Parameter, Spec, Statement};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(crate) struct Entry {
    pub(crate) mutable: bool,
    pub(crate) writable: bool,
    pub(crate) copyable: bool,
    pub(crate) lent: bool,
    pub(crate) nominal: Option<String>,
    pub(crate) moved: bool,
}

#[derive(Clone)]
pub(crate) struct Loan {
    pub(crate) holder: String,
    pub(crate) target: String,
    pub(crate) mutable: bool,
}

#[derive(Clone)]
pub(crate) struct Flow<'a> {
    pub(crate) table: &'a Table,
    pub(crate) vars: BTreeMap<String, Entry>,
    pub(crate) loans: Vec<Loan>,
}

pub(crate) fn verify(table: &Table, inputs: &[Parameter], body: &Body) -> Result<(), Finding> {
    let mut state = Flow {
        table,
        vars: BTreeMap::new(),
        loans: Vec::new(),
    };
    for param in inputs {
        state
            .vars
            .insert(param.name.clone(), parameter(table, param));
    }
    state.body(body)
}

fn parameter(table: &Table, param: &Parameter) -> Entry {
    let Some(spec) = &param.annotation else {
        return Entry {
            copyable: true,
            ..Entry::default()
        };
    };
    let writable = matches!(spec, Spec::Borrow { mutable: true, .. });
    Entry {
        writable,
        copyable: table.copyable(spec),
        lent: writable,
        nominal: nominal(table, spec),
        ..Entry::default()
    }
}

fn nominal(table: &Table, spec: &Spec) -> Option<String> {
    match spec {
        Spec::Named {
            module: None, name, ..
        } => Some(table.resolve(name)),
        Spec::Borrow { target, .. } => nominal(table, target),
        _ => None,
    }
}

impl<'a> Flow<'a> {
    fn body(&mut self, body: &Body) -> Result<(), Finding> {
        match body {
            Body::Block(block) => self.items(&block.items, self.loans.len()),
            Body::Switch(dispatch) => self.dispatch(dispatch),
            Body::Bucket(partition) => self.partition(partition),
            Body::Loop(kind) => self.repetition(kind),
        }
    }

    pub(crate) fn block(&mut self, block: &Block) -> Result<(), Finding> {
        let mut inner = self.clone();
        let pinned = inner.loans.len();
        inner.items(&block.items, pinned)?;
        for (name, entry) in &mut self.vars {
            if let Some(updated) = inner.vars.get(name) {
                entry.moved = updated.moved;
            }
        }
        Ok(())
    }

    fn items(&mut self, list: &[Item], pinned: usize) -> Result<(), Finding> {
        for (index, item) in list.iter().enumerate() {
            let tail = &list[index..];
            let mut slot = 0usize;
            self.loans.retain(|loan| {
                let keep = slot < pinned || Counter::new(&loan.holder).items(tail) > 0;
                slot += 1;
                keep
            });
            self.statement(item)?;
        }
        Ok(())
    }

    fn statement(&mut self, item: &Item) -> Result<(), Finding> {
        let pos = (item.line, item.column);
        match &item.statement {
            Statement::Bind(binding) => self.bind(binding),
            Statement::Assign { target, value, .. } => {
                self.consume(value)?;
                self.mutate(target, pos)
            }
            Statement::For {
                item: name,
                source,
                body,
            } => {
                self.read(source)?;
                let mut inner = self.clone();
                inner.vars.insert(
                    name.clone(),
                    Entry {
                        copyable: true,
                        ..Entry::default()
                    },
                );
                inner.block(body)
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.consume(expr),
            _ => Ok(()),
        }
    }

    fn bind(&mut self, binding: &Binding) -> Result<(), Finding> {
        self.consume(&binding.value)?;
        if let Some((target, mutable)) = self.loaned(&binding.value) {
            self.loans.push(Loan {
                holder: binding.name.clone(),
                target,
                mutable,
            });
        }
        let (copyable, nominal) = self.describe(binding);
        self.vars.insert(
            binding.name.clone(),
            Entry {
                mutable: binding.mutable,
                writable: binding.mutable,
                copyable,
                nominal,
                ..Entry::default()
            },
        );
        Ok(())
    }

    fn describe(&self, binding: &Binding) -> (bool, Option<String>) {
        if let Some(spec) = &binding.annotation {
            return (self.table.copyable(spec), nominal(self.table, spec));
        }
        match &binding.value {
            Expression::Record {
                module: None, name, ..
            } => {
                let resolved = self.table.resolve(name);
                (self.table.copies.contains(&resolved), Some(resolved))
            }
            Expression::Borrow {
                mutable, target, ..
            } => {
                let owner = root(target)
                    .and_then(|r| self.vars.get(&r))
                    .and_then(|e| e.nominal.clone());
                (!*mutable, owner)
            }
            _ => (true, None),
        }
    }

    fn loaned(&self, expr: &Expression) -> Option<(String, bool)> {
        match expr {
            Expression::Borrow {
                mutable, target, ..
            } => Some((root(target)?, *mutable)),
            Expression::Call {
                callee, arguments, ..
            } => Some((arguments.iter().find_map(source)?, self.returned(callee)?)),
            _ => None,
        }
    }

    fn returned(&self, callee: &Expression) -> Option<bool> {
        let sig = match callee {
            Expression::Word { text, .. } => self.table.functions.get(text)?,
            Expression::Member { base, field, .. } => {
                let owner = root(base)
                    .and_then(|r| self.vars.get(&r))
                    .and_then(|e| e.nominal.as_deref())?;
                self.table.method(owner, field)?
            }
            _ => return None,
        };
        match sig.output.as_ref()? {
            Spec::Borrow { mutable, .. } => Some(*mutable),
            spec if contains(spec) => Some(false),
            _ => None,
        }
    }

    fn mutate(&self, target: &Expression, pos: (usize, usize)) -> Result<(), Finding> {
        let Some((name, entry)) = root(target).and_then(|n| self.vars.get(&n).map(|e| (n, e)))
        else {
            return Ok(());
        };
        if entry.moved {
            return Err(Finding {
                code: Code::Moved,
                line: pos.0,
                column: pos.1,
            });
        }
        if !entry.mutable && !entry.writable {
            return Err(Finding {
                code: Code::Mutable,
                line: pos.0,
                column: pos.1,
            });
        }
        self.conflict(&name, true, pos)
    }

    pub(crate) fn conflict(
        &self,
        target: &str,
        mutable: bool,
        pos: (usize, usize),
    ) -> Result<(), Finding> {
        if self
            .loans
            .iter()
            .any(|loan| loan.target == target && (mutable || loan.mutable))
        {
            return Err(Finding {
                code: Code::Conflict,
                line: pos.0,
                column: pos.1,
            });
        }
        Ok(())
    }

    pub(crate) fn consume(&mut self, expr: &Expression) -> Result<(), Finding> {
        if let Expression::Word { text, line, column } = expr {
            return self.transfer(text, (*line, *column));
        }
        self.read(expr)
    }

    pub(crate) fn transfer(&mut self, name: &str, pos: (usize, usize)) -> Result<(), Finding> {
        let Some(entry) = self.vars.get(name) else {
            return Ok(());
        };
        if entry.moved {
            return Err(Finding {
                code: Code::Moved,
                line: pos.0,
                column: pos.1,
            });
        }
        let copy = entry.copyable;
        self.conflict(name, !copy, pos)?;
        if !copy && let Some(slot) = self.vars.get_mut(name) {
            slot.moved = true;
        }
        Ok(())
    }
}
