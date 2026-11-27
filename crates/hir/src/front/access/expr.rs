use super::Access;
use simplex::tree::{Body, Branch, Dispatch, Expression, Loop, Outcome, Partition};
use simplex::{Code, Finding};

impl<'a> Access<'a> {
    pub(super) fn many(&mut self, list: &[Expression]) -> Result<(), Finding> {
        list.iter().try_for_each(|expr| self.expression(expr))
    }

    pub(super) fn origin(&self, expr: &Expression) -> Option<(String, String)> {
        match expr {
            Expression::Group(inner) | Expression::Borrow { target: inner, .. } => {
                self.origin(inner)
            }
            Expression::Word { text, .. } => self.locals.get(text).cloned(),
            Expression::Record {
                module: Some(prefix),
                name,
                ..
            } => Some((prefix.clone(), name.clone())),
            Expression::Call { callee, .. } => {
                let Expression::Member { base, field, .. } = callee.as_ref() else {
                    return None;
                };
                let Expression::Word { text, .. } = base.as_ref() else {
                    return None;
                };
                let kind = self.modules.get(text)?.returns.get(field)?.clone();
                Some((text.clone(), kind))
            }
            _ => None,
        }
    }

    pub(super) fn repetition(&mut self, kind: &Loop) -> Result<(), Finding> {
        match kind {
            Loop::Block(block) => self.child().items(&block.items),
            Loop::Switch(dispatch) => self.dispatch(dispatch),
            Loop::Bucket(partition) => self.partition(partition),
        }
    }

    fn dispatch(&mut self, dispatch: &Dispatch) -> Result<(), Finding> {
        self.expression(&dispatch.subject)?;
        dispatch.arms.iter().try_for_each(|arm| self.branch(arm))
    }

    fn partition(&mut self, partition: &Partition) -> Result<(), Finding> {
        if let Some(subject) = &partition.subject {
            self.expression(subject)?;
        }
        partition.arms.iter().try_for_each(|arm| self.branch(arm))
    }

    fn branch(&mut self, arm: &Branch) -> Result<(), Finding> {
        match &arm.outcome {
            Outcome::Value(expr) | Outcome::Break(Some(expr)) => self.expression(expr),
            Outcome::Break(None) | Outcome::Continue => Ok(()),
            Outcome::Block(block) => self.child().items(&block.items),
        }
    }

    pub(super) fn expression(&mut self, expr: &Expression) -> Result<(), Finding> {
        match expr {
            Expression::Group(inner)
            | Expression::Try(inner)
            | Expression::Unary { operand: inner, .. }
            | Expression::Borrow { target: inner, .. } => self.expression(inner),
            Expression::List { items, .. } => self.many(items),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.expression(base)?;
                self.expression(start)?;
                limit.as_deref().map_or(Ok(()), |end| self.expression(end))
            }
            Expression::Binary { left, right, .. } => {
                self.expression(left)?;
                self.expression(right)
            }
            Expression::Member { .. } => self.member(expr),
            Expression::Call {
                callee,
                generics,
                arguments,
                line,
                column,
                ..
            } => {
                generics
                    .iter()
                    .try_for_each(|s| self.spec(s, (*line, *column)))?;
                self.expression(callee)?;
                self.many(arguments)
            }
            Expression::Meta(block) => self.child().items(&block.items),
            Expression::Record { .. } => self.record(expr),
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            Expression::Function { .. } => self.function(expr),
            _ => Ok(()),
        }
    }

    fn member(&mut self, expr: &Expression) -> Result<(), Finding> {
        let Expression::Member {
            base,
            field,
            generics,
            line,
            column,
            ..
        } = expr
        else {
            return Ok(());
        };
        generics
            .iter()
            .try_for_each(|s| self.spec(s, (*line, *column)))?;
        if let Expression::Word { text, .. } = base.as_ref()
            && !self.locals.contains_key(text)
            && let Some(exports) = self.modules.get(text)
        {
            return match exports.value(field) {
                Some(code) => Err(Finding {
                    code,
                    line: *line,
                    column: *column,
                }),
                None => Ok(()),
            };
        }
        if let Some((prefix, kind)) = self.origin(base)
            && self
                .modules
                .get(&prefix)
                .is_some_and(|e| !e.readable(&kind, field))
        {
            return Err(Finding {
                code: Code::Visibility,
                line: *line,
                column: *column,
            });
        }
        self.expression(base)
    }

    fn record(&mut self, expr: &Expression) -> Result<(), Finding> {
        let Expression::Record {
            module,
            name,
            fields,
            line,
            column,
            ..
        } = expr
        else {
            return Ok(());
        };
        if let Some(prefix) = module
            && let Some(exports) = self.modules.get(prefix)
            && let Some(code) = exports.constructible(name)
        {
            return Err(Finding {
                code,
                line: *line,
                column: *column,
            });
        }
        fields
            .iter()
            .try_for_each(|slot| self.expression(&slot.value))
    }

    fn function(&mut self, expr: &Expression) -> Result<(), Finding> {
        let Expression::Function {
            inputs,
            output,
            body,
            ..
        } = expr
        else {
            return Ok(());
        };
        let mut inner = self.child();
        inner.parameters(inputs, (1, 1))?;
        for param in inputs {
            if let Some(spec) = &param.annotation {
                inner.remember(&param.name, spec);
            }
        }
        if let Some(out) = output {
            inner.spec(out, (1, 1))?;
        }
        match body {
            Body::Block(block) => inner.items(&block.items),
            Body::Switch(dispatch) => inner.dispatch(dispatch),
            Body::Bucket(partition) => inner.partition(partition),
            Body::Loop(kind) => inner.repetition(kind),
        }
    }
}
