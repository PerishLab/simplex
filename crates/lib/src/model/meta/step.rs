use super::eval::{Machine, Value};
use crate::code::Finding;
use crate::tree::{Expression, Item, Loop, Outcome, Pattern, Statement};

pub(super) enum Signal {
    Break(Option<Value>),
    Continue,
    Fault(Finding),
}

impl<'a> Machine<'a> {
    pub(super) fn items(&mut self, list: &[Item]) -> Result<Value, Signal> {
        let mut last = Value::Void;
        for item in list {
            last = self.stmt(item)?;
        }
        Ok(last)
    }

    fn stmt(&mut self, item: &Item) -> Result<Value, Signal> {
        let pos = (item.line, item.column);
        self.tick(pos)?;
        match &item.statement {
            Statement::Bind(b) => {
                let val = self.eval(&b.value)?;
                self.vars.insert(b.name.clone(), val);
                Ok(Value::Void)
            }
            Statement::Assign {
                target,
                operator,
                value,
            } => {
                let rhs = self.eval(value)?;
                if let Expression::Word { text, .. } = target {
                    let updated = assign(operator, self.vars.get(text).cloned(), rhs);
                    self.vars.insert(text.clone(), updated);
                }
                Ok(Value::Void)
            }
            Statement::For {
                item: name,
                source,
                body,
                ..
            } => self.range(name, source, &body.items),
            Statement::Loop(k) => self.repetition(k),
            Statement::Evaluate(e) => self.eval(e),
            _ => Ok(Value::Void),
        }
    }

    fn range(&mut self, name: &str, source: &Expression, body: &[Item]) -> Result<Value, Signal> {
        let Expression::Binary {
            left,
            operator,
            right,
            line,
            column,
            ..
        } = source
        else {
            return Ok(Value::Void);
        };
        if operator != ".." {
            return Ok(Value::Void);
        }
        let (Value::Int(lo), Value::Int(hi)) = (self.eval(left)?, self.eval(right)?) else {
            return Ok(Value::Void);
        };
        for current in lo..hi {
            self.tick((*line, *column))?;
            self.vars.insert(name.to_owned(), Value::Int(current));
            match self.items(body) {
                Ok(_) | Err(Signal::Continue) => {}
                Err(Signal::Break(val)) => return Ok(val.unwrap_or(Value::Void)),
                Err(Signal::Fault(f)) => return Err(Signal::Fault(f)),
            }
        }
        Ok(Value::Void)
    }

    pub(super) fn outcome(&mut self, out: &Outcome) -> Result<Value, Signal> {
        match out {
            Outcome::Value(e) => self.eval(e),
            Outcome::Block(b) => self.items(&b.items),
            Outcome::Break(Some(e)) => Err(Signal::Break(Some(self.eval(e)?))),
            Outcome::Break(None) => Err(Signal::Break(None)),
            Outcome::Continue => Err(Signal::Continue),
        }
    }

    pub(super) fn repetition(&mut self, kind: &Loop) -> Result<Value, Signal> {
        loop {
            let step = match kind {
                Loop::Block(b) => {
                    self.tick((b.line, b.column))?;
                    self.items(&b.items)
                }
                Loop::Switch(d) => self.dispatch(&d.subject, &d.arms, (d.line, d.column)),
                Loop::Bucket(p) => self.partition(p.subject.as_ref(), &p.arms, (p.line, p.column)),
            };
            match step {
                Ok(_) | Err(Signal::Continue) => {}
                Err(Signal::Break(val)) => return Ok(val.unwrap_or(Value::Void)),
                Err(Signal::Fault(f)) => return Err(Signal::Fault(f)),
            }
        }
    }

    pub(super) fn chosen(&mut self, val: &Value, pat: &Pattern) -> Result<bool, Signal> {
        let Pattern::Choices(list) = pat else {
            return Ok(false);
        };
        for item in list {
            if self.eval(item)? == *val {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn matched(&mut self, val: Option<&Value>, pat: &Pattern) -> Result<bool, Signal> {
        match pat {
            Pattern::Wildcard => Ok(true),
            Pattern::Predicate(e) => Ok(self.eval(e)? == Value::Bool(true)),
            Pattern::Choices(_) => match val {
                Some(v) => self.chosen(v, pat),
                None => Ok(false),
            },
            Pattern::Range { start, end } => {
                let Some(Value::Int(n)) = val else {
                    return Ok(false);
                };
                let (Value::Int(lo), Value::Int(hi)) = (self.eval(start)?, self.eval(end)?) else {
                    return Ok(false);
                };
                Ok(*n >= lo && *n < hi)
            }
        }
    }
}

pub(super) fn parse(text: &str) -> i128 {
    if let Some(hex) = text.strip_prefix("0x") {
        i128::from_str_radix(hex, 16).unwrap_or(0)
    } else if let Some(bin) = text.strip_prefix("0b") {
        i128::from_str_radix(bin, 2).unwrap_or(0)
    } else if let Some(oct) = text.strip_prefix("0o") {
        i128::from_str_radix(oct, 8).unwrap_or(0)
    } else {
        text.parse::<i128>().unwrap_or(0)
    }
}

pub(super) fn binary(op: &str, left: Value, right: Value) -> Value {
    match (op, left, right) {
        ("==", a, b) => Value::Bool(a == b),
        ("!=", a, b) => Value::Bool(a != b),
        ("&&", Value::Bool(a), Value::Bool(b)) => Value::Bool(a && b),
        ("||", Value::Bool(a), Value::Bool(b)) => Value::Bool(a || b),
        (_, Value::Int(a), Value::Int(b)) => numeric(op, a, b),
        _ => Value::Void,
    }
}

fn numeric(op: &str, a: i128, b: i128) -> Value {
    match op {
        "+" => Value::Int(a.wrapping_add(b)),
        "-" => Value::Int(a.wrapping_sub(b)),
        "*" => Value::Int(a.wrapping_mul(b)),
        "/" if b != 0 => Value::Int(a / b),
        "%" if b != 0 => Value::Int(a % b),
        "&" => Value::Int(a & b),
        "|" => Value::Int(a | b),
        "^" => Value::Int(a ^ b),
        "<<" => Value::Int(a << (b as u32)),
        ">>" => Value::Int(a >> (b as u32)),
        "<" => Value::Bool(a < b),
        "<=" => Value::Bool(a <= b),
        ">" => Value::Bool(a > b),
        ">=" => Value::Bool(a >= b),
        _ => Value::Void,
    }
}

fn assign(op: &str, current: Option<Value>, rhs: Value) -> Value {
    if op == "=" {
        return rhs;
    }
    let (Some(Value::Int(a)), Value::Int(b)) = (current, rhs) else {
        return Value::Void;
    };
    Value::Int(match op {
        "+=" => a.wrapping_add(b),
        "-=" => a.wrapping_sub(b),
        "*=" => a.wrapping_mul(b),
        "/=" if b != 0 => a / b,
        "%=" if b != 0 => a % b,
        "&=" => a & b,
        "|=" => a | b,
        "^=" => a ^ b,
        "<<=" => a << (b as u32),
        ">>=" => a >> (b as u32),
        _ => a,
    })
}
