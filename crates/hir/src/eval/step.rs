use crate::eval::constant::{Control, Evaluator};
use crate::flow::fallible::qualified;
use crate::front::numeric::{Value, binary, digits, narrow};
use crate::layout::Type;
use simplex::tree::{Body, Branch, Expression, Outcome, Pattern, Spec};
use std::collections::BTreeMap;

impl<'a, 'b> Evaluator<'a, 'b> {
    pub fn step(&mut self, expr: &Expression) -> Result<Value, Control> {
        match expr {
            Expression::Number(text) => Ok(Value::Int(digits(text), Type::Signed(8))),
            Expression::Word { text, .. } => Ok(self.word(text)),
            Expression::Group(inner) | Expression::Try(inner) => self.step(inner),
            Expression::Unary { operator, operand } => {
                let inner = self.step(operand)?;
                Ok(match (operator.as_str(), inner) {
                    ("-", Value::Int(n, k)) => Value::Int(-n, k),
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
                let l = self.step(left)?;
                let r = self.step(right)?;
                Ok(binary(operator, l, r))
            }
            Expression::Member { base, field, .. } => self.member(base, field),
            Expression::Call { .. } => self.call(expr),
            Expression::Meta(block) => self.items(&block.items),
            Expression::Switch(d) => self.dispatch(&d.subject, &d.arms),
            Expression::Bucket(p) => self.partition(p.subject.as_ref(), &p.arms),
            Expression::Loop(k) => self.repetition(k),
            _ => Ok(Value::Void),
        }
    }

    fn word(&self, text: &str) -> Value {
        match text {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            other => self
                .vars
                .get(other)
                .or_else(|| self.globals.get(&(self.stem.clone(), other.to_owned())))
                .cloned()
                .unwrap_or(Value::Void),
        }
    }

    fn member(&mut self, base: &Expression, field: &str) -> Result<Value, Control> {
        if let Expression::Word { text, .. } = base
            && !self.vars.contains_key(text)
            && let Some((target, false)) =
                self.world.imports.get(&(self.stem.clone(), text.clone()))
            && let Some(val) = self.globals.get(&(target.clone(), field.to_owned()))
        {
            return Ok(val.clone());
        }
        if let Some((owner, cname)) = qualified(base, &self.stem, &self.world.imports) {
            let kind = self.typing.nominal(&owner, &cname, &[]);
            let catalog = &self.typing.catalog;
            if let Type::Choice(key) = kind
                && let Some(tagged) = catalog.choices.get(&key)
                && let Some(found) = tagged.arms.iter().find(|a| a.name == field)
            {
                return Ok(Value::Tag {
                    choice: key,
                    arm: field.to_owned(),
                    tag: found.tag,
                });
            }
        }
        self.step(base)
    }

    fn call(&mut self, expr: &Expression) -> Result<Value, Control> {
        let Expression::Call {
            callee,
            generics,
            arguments,
            ..
        } = expr
        else {
            return Ok(Value::Void);
        };
        if let Expression::Member {
            base,
            field,
            generics: mgen,
            ..
        } = callee.as_ref()
        {
            if let Expression::Word { text, .. } = base.as_ref()
                && !self.vars.contains_key(text)
                && let Some((target, false)) = self
                    .world
                    .imports
                    .get(&(self.stem.clone(), text.clone()))
                    .cloned()
            {
                return self.invoke(&target, field, arguments);
            }
            let target = self.step(base)?;
            let chosen = match mgen.is_empty() {
                true => generics,
                false => mgen,
            };
            return Ok(self.postfix(field, target, chosen));
        }
        let Expression::Word { text, .. } = callee.as_ref() else {
            return Ok(Value::Void);
        };
        match text.as_str() {
            "assert" => Ok(Value::Void),
            "has" => self.tagged(arguments),
            "fields" | "variants" | "size" => Ok(self.reflect(text, generics)),
            other => {
                let stem = self.stem.clone();
                self.invoke(&stem, other, arguments)
            }
        }
    }

    fn postfix(&mut self, field: &str, target: Value, generics: &[Spec]) -> Value {
        match (field, target) {
            ("len", Value::Count(n)) => Value::Int(n as i128, Type::Unsigned(8)),
            ("into" | "as", Value::Int(n, _)) => {
                let kind = generics
                    .first()
                    .map(|s| self.typing.resolve(&self.stem, s, &self.subst))
                    .unwrap_or(Type::Signed(8));
                Value::Int(narrow(n, &kind), kind)
            }
            _ => Value::Void,
        }
    }

    fn tagged(&mut self, args: &[Expression]) -> Result<Value, Control> {
        let [first, Expression::Word { text, .. }] = args else {
            return Ok(Value::Bool(false));
        };
        let Value::Field(tags) = self.step(first)? else {
            return Ok(Value::Bool(false));
        };
        Ok(Value::Bool(tags.contains(text)))
    }

    fn reflect(&mut self, kind: &str, generics: &[Spec]) -> Value {
        let Some(spec) = generics.first() else {
            return Value::Void;
        };
        let resolved = self.typing.resolve(&self.stem, spec, &self.subst);
        let catalog = &self.typing.catalog;
        match (kind, &resolved) {
            ("fields", Type::Record(name)) => {
                Value::Count(catalog.records.get(name).map_or(0, |r| r.fields.len()))
            }
            ("variants", Type::Choice(name)) => {
                Value::Count(catalog.choices.get(name).map_or(0, |c| c.arms.len()))
            }
            ("size", _) => Value::Int(resolved.footprint(catalog).size as i128, Type::Unsigned(8)),
            _ => Value::Void,
        }
    }

    fn invoke(&mut self, stem: &str, name: &str, args: &[Expression]) -> Result<Value, Control> {
        let bindings = &self.world.bindings;
        let Some(binding) = bindings.get(&(stem.to_owned(), name.to_owned())).cloned() else {
            return Ok(Value::Void);
        };
        let Expression::Function { inputs, body, .. } = binding.value else {
            return Ok(Value::Void);
        };
        let mut frame = BTreeMap::new();
        for (p, a) in inputs.iter().zip(args) {
            frame.insert(p.name.clone(), self.step(a)?);
        }
        let prevvars = std::mem::replace(&mut self.vars, frame);
        let prevstem = std::mem::replace(&mut self.stem, stem.to_owned());
        let result = match &body {
            Body::Block(b) => self.items(&b.items),
            Body::Switch(d) => self.dispatch(&d.subject, &d.arms),
            Body::Bucket(p) => self.partition(p.subject.as_ref(), &p.arms),
            Body::Loop(k) => self.repetition(k),
        };
        self.stem = prevstem;
        self.vars = prevvars;
        Ok(match result {
            Ok(v) | Err(Control::Break(Some(v))) => v,
            Err(_) => Value::Void,
        })
    }

    pub fn dispatch(&mut self, subject: &Expression, arms: &[Branch]) -> Result<Value, Control> {
        let val = self.step(subject)?;
        for arm in arms {
            if let Pattern::Choices(list) = &arm.pattern
                && list.iter().any(|c| self.eval(c) == val)
            {
                return self.outcome(&arm.outcome);
            }
        }
        Ok(Value::Void)
    }

    pub fn partition(
        &mut self,
        subject: Option<&Expression>,
        arms: &[Branch],
    ) -> Result<Value, Control> {
        let val = match subject {
            Some(s) => Some(self.step(s)?),
            None => None,
        };
        for arm in arms {
            if self.matches(val.as_ref(), &arm.pattern)? {
                return self.outcome(&arm.outcome);
            }
        }
        Ok(Value::Void)
    }

    fn matches(&mut self, val: Option<&Value>, pat: &Pattern) -> Result<bool, Control> {
        match pat {
            Pattern::Wildcard => Ok(true),
            Pattern::Predicate(e) => Ok(self.step(e)? == Value::Bool(true)),
            Pattern::Choices(list) => {
                Ok(val.is_some_and(|v| list.iter().any(|c| self.eval(c) == *v)))
            }
            Pattern::Range { start, end } => {
                let Some(Value::Int(n, _)) = val else {
                    return Ok(false);
                };
                let (Value::Int(lo, _), Value::Int(hi, _)) = (self.step(start)?, self.step(end)?)
                else {
                    return Ok(false);
                };
                Ok(*n >= lo && *n < hi)
            }
        }
    }

    fn outcome(&mut self, out: &Outcome) -> Result<Value, Control> {
        match out {
            Outcome::Value(e) => self.step(e),
            Outcome::Block(b) => self.items(&b.items),
            Outcome::Break(Some(e)) => Err(Control::Break(Some(self.step(e)?))),
            Outcome::Break(None) => Err(Control::Break(None)),
            Outcome::Continue => Err(Control::Continue),
        }
    }
}
