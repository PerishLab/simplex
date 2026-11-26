use super::Signature;
use super::borrow::root;
use super::flow::Flow;
use crate::code::{Code, Finding};
use crate::tree::{Branch, Dispatch, Expression, Loop, Outcome, Partition, Spec};

impl<'a> Flow<'a> {
    pub(crate) fn read(&mut self, expr: &Expression) -> Result<(), Finding> {
        match expr {
            Expression::Word { text, line, column } => self.inspect(text, (*line, *column)),
            Expression::Group(inner) | Expression::Try(inner) => self.consume(inner),
            Expression::Unary { operand, .. } => self.read(operand),
            Expression::Borrow {
                mutable,
                target,
                line,
                column,
            } => self.borrow(*mutable, target, (*line, *column)),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.read(base)?;
                self.read(start)?;
                if let Some(end) = limit {
                    self.read(end)?;
                }
                Ok(())
            }
            Expression::Binary { left, right, .. } => {
                self.read(left)?;
                self.read(right)
            }
            Expression::Member { base, .. } => self.read(base),
            Expression::Call { .. } => self.call(expr),
            Expression::Record { fields, .. } => {
                for field in fields {
                    self.consume(&field.value)?;
                }
                Ok(())
            }
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            _ => Ok(()),
        }
    }

    pub(crate) fn inspect(&self, name: &str, pos: (usize, usize)) -> Result<(), Finding> {
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
        self.conflict(name, false, pos)
    }

    pub(crate) fn borrow(
        &mut self,
        mutable: bool,
        target: &Expression,
        pos: (usize, usize),
    ) -> Result<(), Finding> {
        if let Expression::Index { start, limit, .. } = target {
            self.read(start)?;
            if let Some(end) = limit {
                self.read(end)?;
            }
        }
        let Some(name) = root(target) else {
            return self.read(target);
        };
        let Some(entry) = self.vars.get(&name) else {
            return Ok(());
        };
        if entry.moved {
            return Err(Finding {
                code: Code::Moved,
                line: pos.0,
                column: pos.1,
            });
        }
        if mutable && !entry.mutable && !entry.writable {
            return Err(Finding {
                code: Code::Mutable,
                line: pos.0,
                column: pos.1,
            });
        }
        self.conflict(&name, mutable, pos)
    }

    fn call(&mut self, expr: &Expression) -> Result<(), Finding> {
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
        let mut active: Vec<(String, bool)> = Vec::new();
        let (sig, offset) = self.receiver(callee, (*line, *column), &mut active)?;
        for (index, arg) in arguments.iter().enumerate() {
            let expected = sig
                .as_ref()
                .and_then(|s| s.inputs.get(index + offset))
                .and_then(Option::as_ref);
            self.argument(arg, expected, &mut active)?;
        }
        Ok(())
    }

    fn receiver(
        &mut self,
        callee: &Expression,
        pos: (usize, usize),
        active: &mut Vec<(String, bool)>,
    ) -> Result<(Option<Signature>, usize), Finding> {
        if let Expression::Word { text, .. } = callee {
            let sig = self.table.functions.get(text).map(clone);
            return Ok((sig, 0));
        }
        if let Expression::Member { base, field, .. } = callee
            && let Some(name) = root(base)
            && let Some(owner) = self.vars.get(&name).and_then(|e| e.nominal.clone())
            && let Some(found) = self.table.method(&owner, field).map(clone)
        {
            match found.inputs.first().and_then(Option::as_ref) {
                Some(Spec::Borrow { mutable, .. }) => {
                    self.borrow(*mutable, base, pos)?;
                    overlap(active, &name, *mutable, pos)?;
                    active.push((name, *mutable));
                }
                _ => {
                    overlap(active, &name, true, pos)?;
                    self.transfer(&name, pos)?;
                    active.push((name, true));
                }
            }
            return Ok((Some(found), 1));
        }
        self.read(callee)?;
        Ok((None, 0))
    }

    fn argument(
        &mut self,
        arg: &Expression,
        expected: Option<&Spec>,
        active: &mut Vec<(String, bool)>,
    ) -> Result<(), Finding> {
        if let Expression::Borrow {
            mutable,
            target,
            line,
            column,
        } = arg
        {
            let pos = (*line, *column);
            if matches!(expected, Some(Spec::Borrow { mutable: true, .. })) && !*mutable {
                return Err(Finding {
                    code: Code::Mutable,
                    line: pos.0,
                    column: pos.1,
                });
            }
            self.borrow(*mutable, target, pos)?;
            if let Some(name) = root(target) {
                overlap(active, &name, *mutable, pos)?;
                active.push((name, *mutable));
            }
            return Ok(());
        }
        if let Expression::Word { text, line, column } = arg
            && let Some(Spec::Borrow { mutable, .. }) = expected
            && self.vars.get(text).is_some_and(|e| e.lent)
        {
            let pos = (*line, *column);
            self.inspect(text, pos)?;
            self.conflict(text, *mutable, pos)?;
            overlap(active, text, *mutable, pos)?;
            active.push((text.clone(), *mutable));
            return Ok(());
        }
        if let Expression::Word { text, line, column } = arg {
            overlap(active, text, true, (*line, *column))?;
        }
        self.consume(arg)
    }

    pub(crate) fn repetition(&mut self, kind: &Loop) -> Result<(), Finding> {
        match kind {
            Loop::Block(block) => self.block(block),
            Loop::Switch(dispatch) => self.dispatch(dispatch),
            Loop::Bucket(partition) => self.partition(partition),
        }
    }

    pub(crate) fn dispatch(&mut self, dispatch: &Dispatch) -> Result<(), Finding> {
        self.read(&dispatch.subject)?;
        self.arms(&dispatch.arms)
    }

    pub(crate) fn partition(&mut self, partition: &Partition) -> Result<(), Finding> {
        if let Some(subject) = &partition.subject {
            self.read(subject)?;
        }
        self.arms(&partition.arms)
    }

    fn arms(&mut self, arms: &[Branch]) -> Result<(), Finding> {
        let before = self.vars.clone();
        let mut merged = before.clone();
        for arm in arms {
            let mut branch = self.clone();
            branch.vars.clone_from(&before);
            match &arm.outcome {
                Outcome::Value(expr) | Outcome::Break(Some(expr)) => branch.consume(expr)?,
                Outcome::Block(block) => branch.block(block)?,
                Outcome::Break(None) | Outcome::Continue => {}
            }
            for (name, state) in &branch.vars {
                if state.moved
                    && let Some(slot) = merged.get_mut(name)
                {
                    slot.moved = true;
                }
            }
        }
        self.vars = merged;
        Ok(())
    }
}

fn clone(sig: &Signature) -> Signature {
    Signature {
        generics: sig.generics.clone(),
        inputs: sig.inputs.clone(),
        output: sig.output.clone(),
    }
}

fn overlap(
    active: &[(String, bool)],
    target: &str,
    mutable: bool,
    pos: (usize, usize),
) -> Result<(), Finding> {
    let hit = active
        .iter()
        .any(|(name, write)| name == target && (mutable || *write));
    if hit {
        return Err(Finding {
            code: Code::Conflict,
            line: pos.0,
            column: pos.1,
        });
    }
    Ok(())
}
