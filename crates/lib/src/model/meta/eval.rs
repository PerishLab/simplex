use super::step::{Signal, binary, parse};
use crate::code::{Code, Finding};
use crate::model::Table;
use crate::tree::{Block, Body, Branch, Expression, Spec};
use std::collections::BTreeMap;

const BUDGET: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Value {
    Int(i128),
    Bool(bool),
    Tag(String, String),
    Count(usize),
    Field(Vec<String>),
    Void,
}

pub(super) struct Machine<'a> {
    pub(super) table: &'a Table,
    pub(super) vars: BTreeMap<String, Value>,
    pub(super) steps: usize,
}

impl<'a> Machine<'a> {
    pub(super) fn new(table: &'a Table, vars: BTreeMap<String, Value>) -> Self {
        Self {
            table,
            vars,
            steps: BUDGET,
        }
    }

    pub(super) fn tick(&mut self, pos: (usize, usize)) -> Result<(), Signal> {
        if self.steps == 0 {
            return Err(Signal::Fault(Finding {
                code: Code::Budget,
                line: pos.0,
                column: pos.1,
            }));
        }
        self.steps -= 1;
        Ok(())
    }

    pub(super) fn block(&mut self, block: &Block) -> Result<Value, Finding> {
        match self.items(&block.items) {
            Ok(val) | Err(Signal::Break(Some(val))) => Ok(val),
            Err(Signal::Break(None) | Signal::Continue) => Ok(Value::Void),
            Err(Signal::Fault(f)) => Err(f),
        }
    }

    pub(super) fn expr(&mut self, expr: &Expression) -> Result<Value, Finding> {
        match self.eval(expr) {
            Ok(val) => Ok(val),
            Err(Signal::Fault(f)) => Err(f),
            Err(_) => Ok(Value::Void),
        }
    }

    pub(super) fn eval(&mut self, expr: &Expression) -> Result<Value, Signal> {
        match expr {
            Expression::Number(text) => Ok(Value::Int(parse(text))),
            Expression::Word { text, line, column } => self.word(text, (*line, *column)),
            Expression::Group(inner) | Expression::Try(inner) => self.eval(inner),
            Expression::Unary { operator, operand } => {
                let v = self.eval(operand)?;
                Ok(match (operator.as_str(), v) {
                    ("-", Value::Int(n)) => Value::Int(-n),
                    ("!", Value::Bool(b)) => Value::Bool(!b),
                    _ => Value::Void,
                })
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                Ok(binary(operator, l, r))
            }
            Expression::Member {
                base,
                field,
                line,
                column,
                ..
            } => self.member(base, field, (*line, *column)),
            Expression::Call { .. } => self.call(expr),
            Expression::Meta(block) => self.items(&block.items),
            Expression::Switch(d) => self.dispatch(&d.subject, &d.arms, (d.line, d.column)),
            Expression::Bucket(p) => {
                self.partition(p.subject.as_ref(), &p.arms, (p.line, p.column))
            }
            Expression::Loop(k) => self.repetition(k),
            _ => Ok(Value::Void),
        }
    }

    fn word(&self, text: &str, pos: (usize, usize)) -> Result<Value, Signal> {
        match text {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            other => self.vars.get(other).cloned().ok_or(Signal::Fault(Finding {
                code: Code::Constant,
                line: pos.0,
                column: pos.1,
            })),
        }
    }

    fn member(
        &mut self,
        base: &Expression,
        field: &str,
        pos: (usize, usize),
    ) -> Result<Value, Signal> {
        if field == "await" {
            return Err(Signal::Fault(Finding {
                code: Code::Effect,
                line: pos.0,
                column: pos.1,
            }));
        }
        if let Expression::Word { text, .. } = base
            && text.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        {
            return Ok(Value::Tag(self.table.resolve(text), field.into()));
        }
        self.eval(base)
    }

    fn call(&mut self, expr: &Expression) -> Result<Value, Signal> {
        let Expression::Call {
            callee,
            generics,
            arguments,
            line,
            column,
            ..
        } = expr
        else {
            return Ok(Value::Void);
        };
        let pos = (*line, *column);
        self.tick(pos)?;
        if let Expression::Member { base, field, .. } = callee.as_ref() {
            let v = self.eval(base)?;
            return match (field.as_str(), v) {
                ("len", Value::Count(n)) => Ok(Value::Int(n as i128)),
                ("into" | "as", Value::Int(n)) => Ok(Value::Int(n)),
                _ => Ok(Value::Void),
            };
        }
        let Expression::Word { text, .. } = callee.as_ref() else {
            return Ok(Value::Void);
        };
        match text.as_str() {
            "assert" => self.assertion(arguments, pos),
            "has" => self.tagged(arguments),
            "fields" | "variants" | "size" => self.reflect(text, generics, pos),
            other => self.invoke(other, arguments, pos),
        }
    }

    fn assertion(&mut self, args: &[Expression], pos: (usize, usize)) -> Result<Value, Signal> {
        let Some(first) = args.first() else {
            return Ok(Value::Void);
        };
        if self.eval(first)? == Value::Bool(true) {
            Ok(Value::Void)
        } else {
            Err(Signal::Fault(Finding {
                code: Code::Assert,
                line: pos.0,
                column: pos.1,
            }))
        }
    }

    fn reflect(&self, kind: &str, generics: &[Spec], pos: (usize, usize)) -> Result<Value, Signal> {
        let Some(spec) = generics.first() else {
            return Err(target(pos));
        };
        match kind {
            "fields" => {
                let name = self.table.nominal(spec).ok_or_else(|| target(pos))?;
                let s = self.table.structs.get(&name).ok_or_else(|| target(pos))?;
                Ok(Value::Count(s.fields.len()))
            }
            "variants" => {
                let name = self.table.nominal(spec).ok_or_else(|| target(pos))?;
                let e = self.table.enums.get(&name).ok_or_else(|| target(pos))?;
                Ok(Value::Count(e.variants.len()))
            }
            _ => super::size(self.table, spec)
                .map(|n| Value::Int(n as i128))
                .ok_or_else(|| target(pos)),
        }
    }

    fn invoke(
        &mut self,
        name: &str,
        args: &[Expression],
        pos: (usize, usize),
    ) -> Result<Value, Signal> {
        if self.table.externals.contains(name)
            || self
                .table
                .functions
                .get(name)
                .is_some_and(|s| s.asynchronous)
        {
            return Err(Signal::Fault(Finding {
                code: Code::Effect,
                line: pos.0,
                column: pos.1,
            }));
        }
        let Some((params, body)) = self.table.bodies.get(name).cloned() else {
            return Ok(Value::Void);
        };
        let mut frame = BTreeMap::new();
        for (p, a) in params.iter().zip(args) {
            frame.insert(p.name.clone(), self.eval(a)?);
        }
        let saved = std::mem::replace(&mut self.vars, frame);
        let result = self.body(&body, pos);
        self.vars = saved;
        result
    }

    fn body(&mut self, body: &Body, pos: (usize, usize)) -> Result<Value, Signal> {
        match body {
            Body::Block(b) => self.items(&b.items),
            Body::Switch(d) => self.dispatch(&d.subject, &d.arms, pos),
            Body::Bucket(p) => self.partition(p.subject.as_ref(), &p.arms, pos),
            Body::Loop(k) => self.repetition(k),
        }
    }

    pub(super) fn dispatch(
        &mut self,
        subject: &Expression,
        arms: &[Branch],
        pos: (usize, usize),
    ) -> Result<Value, Signal> {
        self.tick(pos)?;
        let val = self.eval(subject)?;
        for arm in arms {
            if self.chosen(&val, &arm.pattern)? {
                return self.outcome(&arm.outcome);
            }
        }
        Ok(Value::Void)
    }

    pub(super) fn partition(
        &mut self,
        subject: Option<&Expression>,
        arms: &[Branch],
        pos: (usize, usize),
    ) -> Result<Value, Signal> {
        self.tick(pos)?;
        let val = match subject {
            Some(s) => Some(self.eval(s)?),
            None => None,
        };
        for arm in arms {
            if self.matched(val.as_ref(), &arm.pattern)? {
                return self.outcome(&arm.outcome);
            }
        }
        Ok(Value::Void)
    }
}

fn target(pos: (usize, usize)) -> Signal {
    Signal::Fault(Finding {
        code: Code::Target,
        line: pos.0,
        column: pos.1,
    })
}
