use super::graph::Exports;
use crate::code::{Code, Finding};
use crate::tree::{
    Body, Branch, Dispatch, Expression, Item, Loop, Outcome, Partition, Spec, Statement,
};
use std::collections::BTreeMap;

pub(crate) struct Access<'a> {
    modules: &'a BTreeMap<String, Exports>,
    locals: BTreeMap<String, (String, String)>,
}

impl<'a> Access<'a> {
    pub(crate) fn new(modules: &'a BTreeMap<String, Exports>) -> Self {
        Self {
            modules,
            locals: BTreeMap::new(),
        }
    }

    fn child(&self) -> Self {
        Self {
            modules: self.modules,
            locals: self.locals.clone(),
        }
    }

    pub(crate) fn items(&mut self, list: &[Item]) -> Result<(), Finding> {
        for item in list {
            self.statement(item)?;
        }
        Ok(())
    }

    fn many(&mut self, list: &[Expression]) -> Result<(), Finding> {
        for expr in list {
            self.expression(expr)?;
        }
        Ok(())
    }

    fn statement(&mut self, item: &Item) -> Result<(), Finding> {
        let pos = (item.line, item.column);
        match &item.statement {
            Statement::Struct(s) => {
                for field in &s.fields {
                    self.spec(&field.spec, pos)?;
                }
                Ok(())
            }
            Statement::Bind(b) => {
                if let Some(spec) = &b.annotation {
                    self.spec(spec, pos)?;
                    self.remember(&b.name, spec);
                }
                self.expression(&b.value)
            }
            Statement::Conform(conf) => {
                self.spec(&conf.contract, pos)?;
                for entry in &conf.methods {
                    self.expression(&entry.value)?;
                }
                Ok(())
            }
            Statement::Assign { target, value, .. } => {
                self.expression(target)?;
                self.expression(value)
            }
            Statement::For { source, body, .. } => {
                self.expression(source)?;
                self.child().items(&body.items)
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.expression(expr),
            _ => Ok(()),
        }
    }

    fn remember(&mut self, name: &str, spec: &Spec) {
        if let Spec::Named {
            module: Some(prefix),
            name: kind,
            ..
        } = spec
        {
            self.locals
                .insert(name.to_owned(), (prefix.clone(), kind.clone()));
        }
    }

    fn spec(&self, spec: &Spec, pos: (usize, usize)) -> Result<(), Finding> {
        match spec {
            Spec::Named {
                module,
                name,
                arguments,
            } => {
                if let Some(prefix) = module
                    && let Some(exports) = self.modules.get(prefix)
                    && let Some(code) = exports.kind(name)
                {
                    return Err(Finding {
                        code,
                        line: pos.0,
                        column: pos.1,
                    });
                }
                for arg in arguments {
                    self.spec(arg, pos)?;
                }
                Ok(())
            }
            Spec::Borrow { target, .. } | Spec::Slice(target) => self.spec(target, pos),
            Spec::Callable { inputs, output, .. } => {
                for input in inputs {
                    self.spec(input, pos)?;
                }
                if let Some(out) = output {
                    self.spec(out, pos)?;
                }
                Ok(())
            }
        }
    }

    fn repetition(&mut self, kind: &Loop) -> Result<(), Finding> {
        match kind {
            Loop::Block(block) => self.child().items(&block.items),
            Loop::Switch(dispatch) => self.dispatch(dispatch),
            Loop::Bucket(partition) => self.partition(partition),
        }
    }

    fn dispatch(&mut self, dispatch: &Dispatch) -> Result<(), Finding> {
        self.expression(&dispatch.subject)?;
        for arm in &dispatch.arms {
            self.branch(arm)?;
        }
        Ok(())
    }

    fn partition(&mut self, partition: &Partition) -> Result<(), Finding> {
        if let Some(subject) = &partition.subject {
            self.expression(subject)?;
        }
        for arm in &partition.arms {
            self.branch(arm)?;
        }
        Ok(())
    }

    fn branch(&mut self, arm: &Branch) -> Result<(), Finding> {
        match &arm.outcome {
            Outcome::Value(expr) | Outcome::Break(Some(expr)) => self.expression(expr),
            Outcome::Break(None) | Outcome::Continue => Ok(()),
            Outcome::Block(block) => self.child().items(&block.items),
        }
    }

    fn expression(&mut self, expr: &Expression) -> Result<(), Finding> {
        match expr {
            Expression::Group(inner) | Expression::Try(inner) => self.expression(inner),
            Expression::List { items, .. } => self.many(items),
            Expression::Unary { operand, .. } => self.expression(operand),
            Expression::Borrow { target, .. } => self.expression(target),
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
                for spec in generics {
                    self.spec(spec, (*line, *column))?;
                }
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
            line,
            column,
            ..
        } = expr
        else {
            return Ok(());
        };
        if let Expression::Word { text, .. } = base.as_ref() {
            if let Some(exports) = self.modules.get(text)
                && let Some(code) = exports.value(field)
            {
                return Err(Finding {
                    code,
                    line: *line,
                    column: *column,
                });
            }
            if let Some((prefix, kind)) = self.locals.get(text)
                && self
                    .modules
                    .get(prefix)
                    .is_some_and(|e| !e.readable(kind, field))
            {
                return Err(Finding {
                    code: Code::Visibility,
                    line: *line,
                    column: *column,
                });
            }
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
        for field in fields {
            self.expression(&field.value)?;
        }
        Ok(())
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
        for param in inputs {
            if let Some(spec) = &param.annotation {
                inner.spec(spec, (param.line, param.column))?;
                inner.remember(&param.name, spec);
            }
        }
        if let Some(out) = output {
            inner.spec(out, (1, 1))?;
        }
        inner.body(body)
    }

    fn body(&mut self, body: &Body) -> Result<(), Finding> {
        match body {
            Body::Block(block) => self.items(&block.items),
            Body::Switch(dispatch) => self.dispatch(dispatch),
            Body::Bucket(partition) => self.partition(partition),
            Body::Loop(kind) => self.repetition(kind),
        }
    }
}
