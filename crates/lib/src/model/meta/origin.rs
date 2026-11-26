use crate::code::{Code, Finding};
use crate::model::Table;
use crate::tree::{Body, Branch, Expression, Item, Loop, Outcome, Spec, Statement};

impl Table {
    pub(super) fn intrinsics(&self, item: &Item, caller: bool) -> Result<(), Finding> {
        match &item.statement {
            Statement::Bind(b) => {
                let allowed = caller || self.callers.contains(&b.name);
                self.scan(&b.value, allowed)
            }
            Statement::Conform(c) => {
                for entry in &c.methods {
                    self.scan(&entry.value, false)?;
                }
                Ok(())
            }
            Statement::Assign { target, value, .. } => {
                self.scan(target, caller)?;
                self.scan(value, caller)
            }
            Statement::For { source, body, .. } => {
                self.scan(source, caller)?;
                self.sequence(&body.items, caller)
            }
            Statement::Loop(k) => self.looped(k, caller),
            Statement::Evaluate(e) => self.scan(e, caller),
            _ => Ok(()),
        }
    }

    fn sequence(&self, list: &[Item], caller: bool) -> Result<(), Finding> {
        for item in list {
            self.intrinsics(item, caller)?;
        }
        Ok(())
    }

    fn looped(&self, kind: &Loop, caller: bool) -> Result<(), Finding> {
        match kind {
            Loop::Block(b) => self.sequence(&b.items, caller),
            Loop::Switch(d) => {
                self.scan(&d.subject, caller)?;
                self.branches(&d.arms, caller)
            }
            Loop::Bucket(p) => {
                if let Some(s) = &p.subject {
                    self.scan(s, caller)?;
                }
                self.branches(&p.arms, caller)
            }
        }
    }

    fn branches(&self, arms: &[Branch], caller: bool) -> Result<(), Finding> {
        for arm in arms {
            match &arm.outcome {
                Outcome::Value(e) | Outcome::Break(Some(e)) => self.scan(e, caller)?,
                Outcome::Block(b) => self.sequence(&b.items, caller)?,
                Outcome::Break(None) | Outcome::Continue => {}
            }
        }
        Ok(())
    }

    fn scan(&self, expr: &Expression, caller: bool) -> Result<(), Finding> {
        match expr {
            Expression::Group(i) | Expression::Try(i) => self.scan(i, caller),
            Expression::List { items, .. } => {
                for item in items {
                    self.scan(item, caller)?;
                }
                Ok(())
            }
            Expression::Unary { operand, .. }
            | Expression::Borrow {
                target: operand, ..
            } => self.scan(operand, caller),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.scan(base, caller)?;
                self.scan(start, caller)?;
                limit.as_deref().map_or(Ok(()), |e| self.scan(e, caller))
            }
            Expression::Binary { left, right, .. } => {
                self.scan(left, caller)?;
                self.scan(right, caller)
            }
            Expression::Member { base, .. } => self.scan(base, caller),
            Expression::Call { .. } => self.builtin(expr, caller),
            Expression::Meta(b) => self.sequence(&b.items, caller),
            Expression::Record { fields, .. } => {
                for f in fields {
                    self.scan(&f.value, caller)?;
                }
                Ok(())
            }
            Expression::Switch(d) => self.looped(&Loop::Switch(d.clone()), caller),
            Expression::Bucket(p) => self.looped(&Loop::Bucket(p.clone()), caller),
            Expression::Loop(k) => self.looped(k, caller),
            Expression::Function { body, .. } => match body {
                Body::Block(b) => self.sequence(&b.items, caller),
                Body::Switch(d) => self.looped(&Loop::Switch(d.clone()), caller),
                Body::Bucket(p) => self.looped(&Loop::Bucket(p.clone()), caller),
                Body::Loop(k) => self.looped(k, caller),
            },
            _ => Ok(()),
        }
    }

    fn builtin(&self, expr: &Expression, caller: bool) -> Result<(), Finding> {
        let Expression::Call {
            callee,
            generics,
            arguments,
            line,
            column,
            ..
        } = expr
        else {
            return Ok(());
        };
        let pos = (*line, *column);
        if let Expression::Word { text, .. } = callee.as_ref() {
            match text.as_str() {
                "caller" => origin(caller, generics, arguments, pos)?,
                "embed" => confined(generics, arguments, pos)?,
                _ => {}
            }
        }
        self.scan(callee, caller)?;
        for arg in arguments {
            self.scan(arg, caller)?;
        }
        Ok(())
    }
}

fn origin(
    caller: bool,
    generics: &[Spec],
    args: &[Expression],
    pos: (usize, usize),
) -> Result<(), Finding> {
    if !caller || !generics.is_empty() {
        return Err(Finding {
            code: Code::Origin,
            line: pos.0,
            column: pos.1,
        });
    }
    if !args.is_empty() {
        return Err(Finding {
            code: Code::Origin,
            line: pos.0,
            column: pos.1,
        });
    }
    Ok(())
}

fn confined(generics: &[Spec], args: &[Expression], pos: (usize, usize)) -> Result<(), Finding> {
    if !generics.is_empty() || args.len() != 1 {
        return Err(Finding {
            code: Code::Asset,
            line: pos.0,
            column: pos.1,
        });
    }
    let Expression::Text(raw) = &args[0] else {
        return Err(Finding {
            code: Code::Constant,
            line: pos.0,
            column: pos.1,
        });
    };
    let inner = &raw[1..raw.len() - 1];
    if inner.is_empty() || inner.starts_with('/') || inner.contains('\\') {
        return Err(Finding {
            code: Code::Asset,
            line: pos.0,
            column: pos.1,
        });
    }
    if inner
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return Err(Finding {
            code: Code::Asset,
            line: pos.0,
            column: pos.1,
        });
    }
    Ok(())
}
