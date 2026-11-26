use crate::front::numeric::{Value, mutate, retype};
use crate::front::types::{Map, Typing};
use crate::front::world::World;
use crate::layout::Type;
use simplex::tree::{Block, Expression, Item, Loop, Statement};
use std::collections::BTreeMap;

pub enum Control {
    Break(Option<Value>),
    Continue,
}

pub struct Evaluator<'a, 'b> {
    pub world: &'a World,
    pub typing: &'b mut Typing<'a>,
    pub stem: String,
    pub globals: BTreeMap<(String, String), Value>,
    pub vars: BTreeMap<String, Value>,
    pub subst: Map,
}

impl<'a, 'b> Evaluator<'a, 'b> {
    pub fn block(&mut self, block: &Block) -> Value {
        match self.items(&block.items) {
            Ok(val) | Err(Control::Break(Some(val))) => val,
            Err(Control::Break(None) | Control::Continue) => Value::Void,
        }
    }

    pub fn eval(&mut self, expr: &Expression) -> Value {
        self.step(expr).unwrap_or(Value::Void)
    }

    pub fn items(&mut self, list: &[Item]) -> Result<Value, Control> {
        let mut last = Value::Void;
        for item in list {
            last = self.stmt(&item.statement)?;
        }
        Ok(last)
    }

    fn stmt(&mut self, stmt: &Statement) -> Result<Value, Control> {
        match stmt {
            Statement::Bind(b) => {
                let mut val = self.step(&b.value)?;
                if let Some(spec) = &b.annotation {
                    let kind = self.typing.resolve(&self.stem, spec, &self.subst);
                    val = retype(val, kind);
                }
                self.vars.insert(b.name.clone(), val);
                Ok(Value::Void)
            }
            Statement::Assign {
                target,
                operator,
                value,
            } => {
                let rhs = self.step(value)?;
                if let Expression::Word { text, .. } = target {
                    let next = mutate(operator, self.vars.get(text).cloned(), rhs);
                    self.vars.insert(text.clone(), next);
                }
                Ok(Value::Void)
            }
            Statement::For {
                constant,
                item,
                source,
                body,
            } => match constant {
                true => self.unroll(item, source, &body.items),
                false => self.range(item, source, &body.items),
            },
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.step(expr),
            _ => Ok(Value::Void),
        }
    }

    fn unroll(&mut self, item: &str, source: &Expression, body: &[Item]) -> Result<Value, Control> {
        let Expression::Call { generics, .. } = source else {
            return self.range(item, source, body);
        };
        let Some(spec) = generics.first() else {
            return Ok(Value::Void);
        };
        let Type::Record(key) = self.typing.resolve(&self.stem, spec, &self.subst) else {
            return Ok(Value::Void);
        };
        let Some((owner, raw, _)) = self.typing.owner(&key).cloned() else {
            return Ok(Value::Void);
        };
        let structs = &self.world.structs;
        let Some(def) = structs.get(&(owner, raw)).cloned() else {
            return Ok(Value::Void);
        };
        for slot in &def.fields {
            let tags = slot.decorators.iter().map(|d| d.name.clone()).collect();
            self.vars.insert(item.to_owned(), Value::Field(tags));
            self.items(body)?;
        }
        Ok(Value::Void)
    }

    fn range(&mut self, item: &str, source: &Expression, body: &[Item]) -> Result<Value, Control> {
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = source
        else {
            return Ok(Value::Void);
        };
        if operator != ".." {
            return Ok(Value::Void);
        }
        let (Value::Int(lo, kind), Value::Int(hi, _)) = (self.step(left)?, self.step(right)?)
        else {
            return Ok(Value::Void);
        };
        for cur in lo..hi {
            self.vars
                .insert(item.to_owned(), Value::Int(cur, kind.clone()));
            match self.items(body) {
                Ok(_) | Err(Control::Continue) => {}
                Err(Control::Break(val)) => return Ok(val.unwrap_or(Value::Void)),
            }
        }
        Ok(Value::Void)
    }

    pub fn repetition(&mut self, kind: &Loop) -> Result<Value, Control> {
        loop {
            let step = match kind {
                Loop::Block(b) => self.items(&b.items),
                Loop::Switch(d) => self.dispatch(&d.subject, &d.arms),
                Loop::Bucket(p) => self.partition(p.subject.as_ref(), &p.arms),
            };
            match step {
                Ok(_) | Err(Control::Continue) => {}
                Err(Control::Break(v)) => return Ok(v.unwrap_or(Value::Void)),
            }
        }
    }
}
