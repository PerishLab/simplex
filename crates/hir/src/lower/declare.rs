use crate::front::numeric::{Value, digits, encode};
use crate::front::types::Map;
use crate::layout::{Mangle, Type};
use crate::lower::engine::{Engine, Spec, Task};
use crate::lower::frame::Frame;
use crate::tree::{Expression, Routine, Statement};
use simplex::tree as syntax;
use std::collections::BTreeMap;

impl<'a> Engine<'a> {
    pub fn declarations(&mut self) {
        let last = self.world.order.last().cloned().unwrap_or_default();
        for stem in self.world.order.clone() {
            let Some(items) = self.world.programs.get(&stem).cloned() else {
                continue;
            };
            let mut loose = Vec::new();
            for item in &items {
                if self.collect(&stem, &last, item) {
                    loose.push(item.clone());
                }
            }
            self.script(&stem, &loose);
        }
    }

    fn collect(&mut self, stem: &str, entry: &str, item: &syntax::Item) -> bool {
        match &item.statement {
            syntax::Statement::Bind(b) => !self.top(stem, entry, b),
            syntax::Statement::Assign { .. }
            | syntax::Statement::For { .. }
            | syntax::Statement::Loop(_) => true,
            syntax::Statement::Evaluate(e) => !matches!(e, syntax::Expression::Meta(_)),
            _ => false,
        }
    }

    fn script(&mut self, stem: &str, loose: &[syntax::Item]) {
        if loose.is_empty() {
            return;
        }
        let sym = Mangle::symbol(&self.ns, stem, "init");
        if self.seen.insert(sym.clone()) {
            let mut frame = Frame::new(self, stem, Type::Void, BTreeMap::new());
            let body = frame.script(loose);
            self.routines.push(Routine {
                symbol: sym,
                exported: false,
                asynchronous: false,
                inputs: Vec::new(),
                output: Type::Void,
                body,
            });
        }
    }

    fn top(&mut self, stem: &str, entry: &str, b: &syntax::Binding) -> bool {
        let key = (stem.to_owned(), b.name.clone());
        if self.globals.contains_key(&key) || self.embedded.contains_key(&key) {
            return true;
        }
        let syntax::Expression::Function {
            generics, inputs, ..
        } = &b.value
        else {
            if let syntax::Expression::Number(n) = &b.value {
                let raw = self
                    .typing
                    .optional(stem, b.annotation.as_ref(), &Map::new());
                let kind = match raw {
                    Type::Void => Type::Signed(8),
                    other => other,
                };
                self.globals.insert(key, Value::Int(digits(n), kind));
            }
            return false;
        };
        if !generics.is_empty() || inputs.iter().any(|p| p.constant) {
            return true;
        }
        let sym = Mangle::symbol(&self.ns, stem, &b.name);
        if self.seen.insert(sym.clone()) {
            let exported = match self.shared {
                true => b.dynamic,
                false => b.public || (stem == entry && b.name == "main"),
            };
            let routine = self.decorated((stem, sym), exported, b);
            self.routines.push(routine);
        }
        true
    }

    pub fn ensure(&mut self, stem: &str, name: &str) -> Option<Type> {
        let sym = Mangle::symbol(&self.ns, stem, name);
        if let Some(found) = self.routines.iter().find(|r| r.symbol == sym) {
            return Some(found.output.clone());
        }
        let bindings = &self.world.bindings;
        let b = bindings.get(&(stem.to_owned(), name.to_owned())).cloned()?;
        if self.seen.insert(sym.clone()) {
            let last = self.world.order.last().cloned().unwrap_or_default();
            let exported = match self.shared {
                true => b.dynamic,
                false => b.public || (stem == last && b.name == "main"),
            };
            let routine = self.decorated((stem, sym), exported, &b);
            let out = routine.output.clone();
            self.routines.push(routine);
            return Some(out);
        }
        None
    }

    pub fn decorated(
        &mut self,
        loc: (&str, String),
        exported: bool,
        b: &syntax::Binding,
    ) -> Routine {
        let (stem, symbol) = loc;
        let (empty, none) = (Map::new(), BTreeMap::new());
        let spec = Spec {
            expr: &b.value,
            subst: &empty,
            constants: &none,
            exported,
        };
        let mut routine = self.callable((stem, symbol), &spec);
        let callers = &self.world.callers;
        if callers.contains(&(stem.to_owned(), b.name.clone())) {
            let origin = self.typing.origin();
            routine.inputs.push((String::from("$caller"), origin));
        }
        for dec in &b.decorators {
            if dec.module.is_none() && !matches!(dec.name.as_str(), "caller" | "derive") {
                routine = self.wrap((stem, &b.name), dec, routine);
            }
        }
        routine
    }

    fn wrap(
        &mut self,
        loc: (&str, &str),
        dec: &syntax::Decorator,
        mut routine: Routine,
    ) -> Routine {
        let (stem, target) = loc;
        let wsym = Mangle::symbol(&self.ns, stem, &dec.name);
        let kind = self.typing.nominal(stem, "Span", &[]);
        let label = self
            .pool
            .intern((&self.ns, stem), target.as_bytes().to_vec());
        let span = Expression::Record {
            name: encode(&kind),
            fields: vec![
                (String::from("name"), Expression::Bytes(label)),
                (
                    String::from("line"),
                    Expression::Int(dec.line as i128, Type::Unsigned(8)),
                ),
                (
                    String::from("column"),
                    Expression::Int(dec.column as i128, Type::Unsigned(8)),
                ),
            ],
            kind,
        };
        let mut extra = Vec::new();
        {
            let mut frame = Frame::new(self, stem, routine.output.clone(), BTreeMap::new());
            for arg in &dec.arguments {
                extra.push(frame.expr(arg, None).0);
            }
        }
        if let Some(Statement::Result(last)) = routine.body.pop() {
            extra.push(span);
            extra.push(last);
            routine.body.push(Statement::Result(Expression::Call {
                symbol: wsym,
                arguments: extra,
                kind: routine.output.clone(),
            }));
        }
        routine
    }

    pub fn callable(&mut self, loc: (&str, String), spec: &Spec<'_>) -> Routine {
        let (stem, symbol) = loc;
        let syntax::Expression::Function {
            asynchronous,
            inputs,
            output,
            body,
            ..
        } = spec.expr
        else {
            return Routine {
                symbol,
                exported: spec.exported,
                asynchronous: false,
                inputs: Vec::new(),
                output: Type::Void,
                body: Vec::new(),
            };
        };
        let params: Vec<(String, Type)> = inputs
            .iter()
            .filter(|p| !p.constant)
            .map(|p| {
                let raw = self
                    .typing
                    .optional(stem, p.annotation.as_ref(), spec.subst);
                let kind = match raw {
                    Type::Void => Type::Signed(8),
                    other => other,
                };
                (p.name.clone(), kind)
            })
            .collect();
        let ret = self.typing.optional(stem, output.as_ref(), spec.subst);
        let mut frame = Frame::with(
            self,
            stem,
            (ret.clone(), spec.subst.clone()),
            spec.constants.clone(),
        );
        let (stmts, inferred) = frame.function(&params, body);
        let chosen = match ret {
            Type::Void => inferred,
            other => other,
        };
        Routine {
            symbol,
            exported: spec.exported,
            asynchronous: *asynchronous,
            inputs: params,
            output: chosen,
            body: stmts,
        }
    }

    pub fn specialize(&mut self, task: Task) {
        let bindings = &self.world.bindings;
        let Some(b) = bindings
            .get(&(task.stem.clone(), task.name.clone()))
            .cloned()
        else {
            return;
        };
        let spec = Spec {
            expr: &b.value,
            subst: &task.subst,
            constants: &task.constants,
            exported: task.exported,
        };
        let mut routine = self.callable((&task.stem, task.symbol), &spec);
        if self.world.callers.contains(&(task.stem, task.name)) {
            let origin = self.typing.origin();
            routine.inputs.push((String::from("$caller"), origin));
        }
        self.routines.push(routine);
    }
}
