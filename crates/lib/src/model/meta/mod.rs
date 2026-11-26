mod eval;
mod step;
mod unroll;

use self::eval::{Machine, Value};
use super::Table;
use crate::code::{Code, Finding};
use crate::tree::{Body, Expression, Item, Parameter, Program, Spec, Statement};
use std::collections::BTreeMap;

impl Table {
    pub(crate) fn evaluate(&self, program: &Program) -> Result<(), Finding> {
        let mut globals = BTreeMap::new();
        for item in &program.items {
            self.top(item, &mut globals)?;
        }
        let mut inspector = Inspector {
            table: self,
            globals: &globals,
            locals: BTreeMap::new(),
        };
        inspector.items(&program.items)
    }

    fn top(&self, item: &Item, globals: &mut BTreeMap<String, Value>) -> Result<(), Finding> {
        match &item.statement {
            Statement::Bind(b) => {
                if let Expression::Meta(block) = &b.value {
                    let val = Machine::new(self, globals.clone()).block(block)?;
                    globals.insert(b.name.clone(), val);
                }
                Ok(())
            }
            Statement::Evaluate(Expression::Meta(block)) => {
                Machine::new(self, globals.clone()).block(block)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

struct Inspector<'a> {
    table: &'a Table,
    globals: &'a BTreeMap<String, Value>,
    locals: BTreeMap<String, Option<String>>,
}

impl<'a> Inspector<'a> {
    fn child(&self) -> Self {
        Self {
            table: self.table,
            globals: self.globals,
            locals: self.locals.clone(),
        }
    }

    fn items(&mut self, list: &[Item]) -> Result<(), Finding> {
        for item in list {
            self.stmt(item)?;
        }
        Ok(())
    }

    fn stmt(&mut self, item: &Item) -> Result<(), Finding> {
        match &item.statement {
            Statement::Bind(b) => {
                if !matches!(b.value, Expression::Meta(_)) {
                    self.expr(&b.value, false)?;
                }
                let nom = b.annotation.as_ref().and_then(|s| self.table.nominal(s));
                self.locals.insert(b.name.clone(), nom);
                Ok(())
            }
            Statement::Conform(c) => {
                for entry in &c.methods {
                    self.expr(&entry.value, false)?;
                }
                Ok(())
            }
            Statement::Assign { target, value, .. } => {
                self.expr(target, false)?;
                self.expr(value, false)
            }
            Statement::For {
                constant,
                source,
                body,
                ..
            } => {
                if *constant {
                    self.table.direct(source, (item.line, item.column))?;
                } else {
                    self.expr(source, false)?;
                }
                self.child().items(&body.items)
            }
            Statement::Loop(k) => self.repetition(k, false),
            Statement::Evaluate(e) => {
                if !matches!(e, Expression::Meta(_)) {
                    self.expr(e, false)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn repetition(&mut self, kind: &crate::tree::Loop, parameterized: bool) -> Result<(), Finding> {
        match kind {
            crate::tree::Loop::Block(b) => self.block(b, parameterized),
            crate::tree::Loop::Switch(d) => {
                self.expr(&d.subject, parameterized)?;
                self.arms(&d.arms, parameterized)
            }
            crate::tree::Loop::Bucket(p) => {
                if let Some(s) = &p.subject {
                    self.expr(s, parameterized)?;
                }
                self.arms(&p.arms, parameterized)
            }
        }
    }

    fn arms(&mut self, arms: &[crate::tree::Branch], parameterized: bool) -> Result<(), Finding> {
        for arm in arms {
            match &arm.outcome {
                crate::tree::Outcome::Value(e) | crate::tree::Outcome::Break(Some(e)) => {
                    self.expr(e, parameterized)?;
                }
                crate::tree::Outcome::Block(b) => self.block(b, parameterized)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn block(&mut self, block: &crate::tree::Block, parameterized: bool) -> Result<(), Finding> {
        let mut inner = self.child();
        for item in &block.items {
            if let Statement::Evaluate(Expression::Meta(meta)) = &item.statement {
                if !parameterized {
                    Machine::new(self.table, self.globals.clone()).block(meta)?;
                }
            } else {
                inner.stmt(item)?;
            }
        }
        Ok(())
    }

    fn expr(&mut self, expr: &Expression, parameterized: bool) -> Result<(), Finding> {
        match expr {
            Expression::Group(inner) | Expression::Try(inner) => self.expr(inner, parameterized),
            Expression::List { items, .. } => self.many(items, parameterized),
            Expression::Unary { operand, .. }
            | Expression::Borrow {
                target: operand, ..
            } => self.expr(operand, parameterized),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.expr(base, parameterized)?;
                self.expr(start, parameterized)?;
                limit
                    .as_deref()
                    .map_or(Ok(()), |e| self.expr(e, parameterized))
            }
            Expression::Binary { left, right, .. } => {
                self.expr(left, parameterized)?;
                self.expr(right, parameterized)
            }
            Expression::Member { base, .. } => self.expr(base, parameterized),
            Expression::Call { .. } => self.call(expr, parameterized),
            Expression::Meta(block) => {
                if !parameterized {
                    Machine::new(self.table, self.globals.clone()).block(block)?;
                }
                Ok(())
            }
            Expression::Record { fields, .. } => {
                for f in fields {
                    self.expr(&f.value, parameterized)?;
                }
                Ok(())
            }
            Expression::Function { inputs, body, .. } => self.function(inputs, body),
            _ => Ok(()),
        }
    }

    fn many(&mut self, list: &[Expression], parameterized: bool) -> Result<(), Finding> {
        for item in list {
            self.expr(item, parameterized)?;
        }
        Ok(())
    }

    fn function(&self, inputs: &[Parameter], body: &Body) -> Result<(), Finding> {
        let parameterized = inputs.iter().any(|p| p.constant);
        let mut inner = self.child();
        for p in inputs {
            let nom = p.annotation.as_ref().and_then(|s| self.table.nominal(s));
            inner.locals.insert(p.name.clone(), nom);
        }
        match body {
            Body::Block(b) => inner.block(b, parameterized),
            Body::Switch(d) => {
                inner.repetition(&crate::tree::Loop::Switch(d.clone()), parameterized)
            }
            Body::Bucket(p) => {
                inner.repetition(&crate::tree::Loop::Bucket(p.clone()), parameterized)
            }
            Body::Loop(k) => inner.repetition(k, parameterized),
        }
    }

    fn call(&mut self, expr: &Expression, parameterized: bool) -> Result<(), Finding> {
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
        self.table.intrinsic(callee, generics, pos)?;
        if let Expression::Word { text, .. } = callee.as_ref() {
            self.specialize(text, arguments, pos)?;
            self.table.specialize(&self.locals, expr)?;
        }
        self.expr(callee, parameterized)?;
        self.many(arguments, parameterized)
    }

    fn specialize(
        &self,
        name: &str,
        args: &[Expression],
        pos: (usize, usize),
    ) -> Result<(), Finding> {
        let Some((params, body)) = self.table.bodies.get(name) else {
            return Ok(());
        };
        if !params.iter().any(|p| p.constant) {
            return Ok(());
        }
        let mut env = self.globals.clone();
        for (param, arg) in params.iter().zip(args) {
            if !param.constant {
                continue;
            }
            let val = Machine::new(self.table, self.globals.clone())
                .expr(arg)
                .map_err(|f| {
                    if f.code == Code::Constant {
                        Finding {
                            code: Code::Constant,
                            line: pos.0,
                            column: pos.1,
                        }
                    } else {
                        f
                    }
                })?;
            env.insert(param.name.clone(), val);
        }
        self.table.assertions(&env, body)
    }
}

pub(super) fn size(table: &Table, spec: &Spec) -> Option<usize> {
    match spec {
        Spec::Named {
            module: None, name, ..
        } => match name.as_str() {
            "u8" | "i8" | "bool" => Some(1),
            "u16" | "i16" => Some(2),
            "u32" | "i32" | "f32" => Some(4),
            "u64" | "i64" | "usize" | "isize" | "f64" => Some(8),
            other => {
                let s = table.structs.get(&table.resolve(other))?;
                s.fields.iter().map(|f| size(table, &f.spec)).sum()
            }
        },
        _ => None,
    }
}
