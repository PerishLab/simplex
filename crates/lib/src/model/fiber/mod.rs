mod query;

pub(crate) use query::{mentions, spawns};

use self::query::launches;
use super::Table;
use super::count::Counter;
use crate::code::{Code, Finding};
use crate::tree::{
    Binding, Branch, Enumeration, Expression, Item, Loop, Outcome, Program, Statement, Structure,
};
use std::collections::BTreeSet;

impl Table {
    pub(crate) fn fibers(&self, program: &Program) -> Result<(), Finding> {
        for item in &program.items {
            self.header(item)?;
        }
        let ctx = Context {
            table: self,
            immediate: false,
            nursery: false,
            fibers: BTreeSet::new(),
            targets: BTreeSet::new(),
            tainted: BTreeSet::new(),
        };
        ctx.top(&program.items)
    }

    fn header(&self, item: &Item) -> Result<(), Finding> {
        let invalid = match &item.statement {
            Statement::Struct(Structure { fields, .. }) => fields.iter().any(|s| mentions(&s.spec)),
            Statement::Enum(Enumeration { variants, .. }) => {
                variants.iter().flat_map(|v| &v.payloads).any(mentions)
            }
            Statement::Interface(c) => c
                .methods
                .iter()
                .any(|req| req.output.as_ref().is_some_and(mentions)),
            _ => false,
        };
        if invalid {
            return Err(Finding {
                code: Code::Detached,
                line: item.line,
                column: item.column,
            });
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct Context<'a> {
    pub(super) table: &'a Table,
    pub(super) immediate: bool,
    pub(super) nursery: bool,
    pub(super) fibers: BTreeSet<String>,
    pub(super) targets: BTreeSet<String>,
    pub(super) tainted: BTreeSet<String>,
}

impl<'a> Context<'a> {
    fn top(&self, list: &[Item]) -> Result<(), Finding> {
        for item in list {
            self.entry(&item.statement)?;
        }
        Ok(())
    }

    fn entry(&self, stmt: &Statement) -> Result<(), Finding> {
        match stmt {
            Statement::Bind(b) => self.expr(&b.value),
            Statement::Conform(c) => {
                for entry in &c.methods {
                    self.expr(&entry.value)?;
                }
                Ok(())
            }
            Statement::Evaluate(e) => self.expr(e),
            _ => Ok(()),
        }
    }

    pub(super) fn items(&mut self, list: &[Item]) -> Result<(), Finding> {
        for (index, item) in list.iter().enumerate() {
            self.stmt(item, &list[index + 1..], index + 1 == list.len())?;
        }
        Ok(())
    }

    fn stmt(&mut self, item: &Item, rest: &[Item], tail: bool) -> Result<(), Finding> {
        let pos = (item.line, item.column);
        match &item.statement {
            Statement::Bind(b) => self.bind(b, rest, pos),
            Statement::Assign { target, value, .. } => {
                self.expr(target)?;
                self.expr(value)
            }
            Statement::For { source, body, .. } => {
                self.expr(source)?;
                self.clone().items(&body.items)
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => {
                self.expr(expr)?;
                if !(tail && !self.immediate && self.nursery) && self.pending(expr) {
                    return Err(Finding {
                        code: Code::Detached,
                        line: pos.0,
                        column: pos.1,
                    });
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn bind(&mut self, b: &Binding, rest: &[Item], pos: (usize, usize)) -> Result<(), Finding> {
        self.expr(&b.value)?;
        if b.mutable {
            self.targets.insert(b.name.clone());
        }
        if b.annotation.as_ref().is_some_and(mentions) || self.pending(&b.value) {
            if Counter::new(&b.name).items(rest) == 0 {
                return Err(Finding {
                    code: Code::Detached,
                    line: pos.0,
                    column: pos.1,
                });
            }
            self.fibers.insert(b.name.clone());
            if self.dirty(&b.value) {
                self.tainted.insert(b.name.clone());
            }
        }
        Ok(())
    }

    pub(super) fn repetition(&mut self, kind: &Loop) -> Result<(), Finding> {
        match kind {
            Loop::Block(block) => self.clone().items(&block.items),
            Loop::Switch(d) => {
                self.expr(&d.subject)?;
                self.arms(&d.arms)
            }
            Loop::Bucket(p) => {
                if let Some(sub) = &p.subject {
                    self.expr(sub)?;
                }
                self.arms(&p.arms)
            }
        }
    }

    fn arms(&mut self, arms: &[Branch]) -> Result<(), Finding> {
        for arm in arms {
            let mut inner = self.clone();
            match &arm.outcome {
                Outcome::Value(e) | Outcome::Break(Some(e)) => inner.expr(e)?,
                Outcome::Block(b) => inner.items(&b.items)?,
                Outcome::Break(None) | Outcome::Continue => {}
            }
        }
        Ok(())
    }

    fn expr(&self, expr: &Expression) -> Result<(), Finding> {
        match expr {
            Expression::Group(inner) | Expression::Try(inner) => self.expr(inner),
            Expression::List { items, .. } => self.many(items),
            Expression::Unary { operand, .. }
            | Expression::Borrow {
                target: operand, ..
            } => self.expr(operand),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.expr(base)?;
                self.expr(start)?;
                limit.as_deref().map_or(Ok(()), |e| self.expr(e))
            }
            Expression::Binary { left, right, .. } => {
                self.expr(left)?;
                self.expr(right)
            }
            Expression::Member {
                base,
                field,
                line,
                column,
                ..
            } => {
                if field == "await" && !self.immediate {
                    return Err(Finding {
                        code: Code::Context,
                        line: *line,
                        column: *column,
                    });
                }
                self.expr(base)
            }
            Expression::Call { .. } => self.call(expr),
            Expression::Meta(block) => self.clone().items(&block.items),
            Expression::Record { fields, .. } => {
                for f in fields {
                    self.expr(&f.value)?;
                }
                Ok(())
            }
            Expression::Switch(d) => self.clone().repetition(&Loop::Switch(d.clone())),
            Expression::Bucket(p) => self.clone().repetition(&Loop::Bucket(p.clone())),
            Expression::Loop(k) => self.clone().repetition(k),
            Expression::Function { .. } => self.function(expr),
            _ => Ok(()),
        }
    }

    fn many(&self, items: &[Expression]) -> Result<(), Finding> {
        for item in items {
            self.expr(item)?;
        }
        Ok(())
    }

    fn call(&self, expr: &Expression) -> Result<(), Finding> {
        let Expression::Call {
            callee,
            arguments,
            line,
            column,
            ..
        } = expr
        else {
            return Ok(());
        };
        if !self.nursery && launches(self.table, callee) {
            return Err(Finding {
                code: Code::Context,
                line: *line,
                column: *column,
            });
        }
        if let Expression::Member { base, field, .. } = callee.as_ref()
            && field == "race"
            && self.dirty(base)
        {
            return Err(Finding {
                code: Code::Cancel,
                line: *line,
                column: *column,
            });
        }
        self.expr(callee)?;
        self.many(arguments)
    }
}
