use crate::front::numeric::{Value, encode};
use crate::front::types::Map;
use crate::layout::Type;
use crate::lower::engine::Task;
use crate::lower::frame::Frame;
use crate::tree::Expression;
use simplex::tree as syntax;
use std::collections::BTreeMap;

type Prepared = Vec<(Expression, Type, Option<syntax::Spec>)>;

struct Bound {
    lowered: Prepared,
    constants: BTreeMap<String, Value>,
    suffix: Vec<String>,
}

impl<'b, 'a> Frame<'b, 'a> {
    pub fn construct(
        &mut self,
        owner: (&str, &str),
        arm: &str,
        args: &[syntax::Expression],
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let (_, resolved) = self.engine.world.unalias(owner.0, owner.1);
        let raw = match &resolved {
            syntax::Spec::Named { name, .. } => name.clone(),
            _ => owner.1.to_owned(),
        };
        let mut lowered: Vec<(Expression, Type)> =
            args.iter().map(|a| self.expr(a, None)).collect();
        let kind = match expected {
            Some(Type::Choice(k)) => Type::Choice(k.clone()),
            _ => self.variant(owner.0, &raw, arm, &lowered),
        };
        let key = match &kind {
            Type::Choice(k) => k.clone(),
            _ => raw,
        };
        let adef = self
            .engine
            .typing
            .catalog
            .choices
            .get(&key)
            .and_then(|c| c.arms.iter().find(|a| a.name == arm))
            .cloned();
        let tag = adef.as_ref().map_or(0, |a| a.tag);
        let mut payloads = Vec::new();
        for (idx, (val, vk)) in lowered.drain(..).enumerate() {
            let coerced = match adef.as_ref().and_then(|a| a.payloads.get(idx)) {
                Some(slot) => self.coerce(val, &vk, &slot.kind),
                None => val,
            };
            payloads.push(coerced);
        }
        (
            Expression::Variant {
                choice: key,
                arm: arm.to_owned(),
                tag,
                payloads,
                kind: kind.clone(),
            },
            kind,
        )
    }

    fn variant(
        &mut self,
        owner: &str,
        raw: &str,
        arm: &str,
        lowered: &[(Expression, Type)],
    ) -> Type {
        let enums = &self.engine.world.enums;
        let Some(def) = enums.get(&(owner.to_owned(), raw.to_owned())).cloned() else {
            return self.engine.typing.nominal(owner, raw, &[]);
        };
        let vdef = def.variants.iter().find(|v| v.name == arm).cloned();
        let gargs: Vec<Type> = def
            .generics
            .iter()
            .map(|g| match &vdef {
                Some(v) => matched(&v.payloads, &g.name, lowered),
                None => Type::Unsigned(8),
            })
            .collect();
        self.engine.typing.nominal(owner, raw, &gargs)
    }

    pub fn direct(
        &mut self,
        stem: &str,
        name: &str,
        call: (&[syntax::Spec], &[syntax::Expression]),
        pos: (usize, usize),
    ) -> (Expression, Type) {
        if stem == self.stem
            && let Some(Type::Function { inputs, output }) = self.lookup(name)
        {
            let args = self.arguments(call.1, &inputs);
            let fkind = Type::Function {
                inputs,
                output: output.clone(),
            };
            return (
                Expression::Indirect {
                    callee: Box::new(Expression::Local(name.to_owned(), fkind)),
                    arguments: args,
                    kind: *output.clone(),
                },
                *output,
            );
        }
        let externals = &self.engine.world.externals;
        if let Some(ext) = externals.get(&(stem.to_owned(), name.to_owned())).cloned() {
            let ins: Vec<Type> = ext
                .inputs
                .iter()
                .map(|p| {
                    self.engine
                        .typing
                        .optional(stem, p.annotation.as_ref(), &Map::new())
                })
                .collect();
            let out = self
                .engine
                .typing
                .optional(stem, ext.output.as_ref(), &Map::new());
            let args = self.arguments(call.1, &ins);
            let sym = crate::eval::asset::trim(&ext.symbol);
            return (
                Expression::Call {
                    symbol: sym,
                    arguments: args,
                    kind: out.clone(),
                },
                out,
            );
        }
        let bindings = &self.engine.world.bindings;
        let Some(b) = bindings.get(&(stem.to_owned(), name.to_owned())).cloned() else {
            let args: Vec<Expression> = call.1.iter().map(|a| self.expr(a, None).0).collect();
            let sym = self.engine.symbol(stem, name);
            return (
                Expression::Call {
                    symbol: sym,
                    arguments: args,
                    kind: Type::Unsigned(8),
                },
                Type::Unsigned(8),
            );
        };
        self.specialized(stem, &b, call, pos)
    }

    fn specialized(
        &mut self,
        stem: &str,
        b: &syntax::Binding,
        call: (&[syntax::Spec], &[syntax::Expression]),
        pos: (usize, usize),
    ) -> (Expression, Type) {
        let syntax::Expression::Function {
            asynchronous,
            generics,
            inputs,
            output,
            ..
        } = &b.value
        else {
            return (Expression::Void, Type::Void);
        };
        let (explicit, args) = call;
        let mut subst = Map::new();
        for (g, spec) in generics.iter().zip(explicit) {
            let resolved = self.engine.typing.resolve(&self.stem, spec, &self.subst);
            subst.insert(g.name.clone(), resolved);
        }
        let mut bound = self.bindargs(stem, (inputs, generics), args, &mut subst);
        for g in generics {
            let chosen = subst.get(&g.name).cloned().unwrap_or(Type::Unsigned(8));
            bound.suffix.insert(0, encode(&chosen));
            subst.entry(g.name.clone()).or_insert(chosen);
        }
        let mut runtime = self.coerceargs(stem, bound.lowered, &subst);
        let custom = !generics.is_empty() || !bound.constants.is_empty();
        let base = self.engine.symbol(stem, &b.name);
        let sym = match custom {
            true => format!("{base}${}", bound.suffix.join("$")),
            false => base,
        };
        if custom {
            let task = Task {
                stem: stem.to_owned(),
                name: b.name.clone(),
                symbol: sym.clone(),
                subst: subst.clone(),
                constants: bound.constants,
                exported: false,
            };
            if self.engine.seen.insert(sym.clone()) {
                self.engine.specialize(task);
            }
        } else if output.is_none() {
            self.engine.ensure(stem, &b.name);
        }
        self.provenance((stem, &b.name), pos, &mut runtime);
        let raw = match output {
            Some(spec) => self.engine.typing.resolve(stem, spec, &subst),
            None => self
                .engine
                .routines
                .iter()
                .find(|r| r.symbol == sym)
                .map_or(Type::Void, |r| r.output.clone()),
        };
        let out = match asynchronous {
            true => Type::Fiber(Box::new(raw)),
            false => raw,
        };
        (
            Expression::Call {
                symbol: sym,
                arguments: runtime,
                kind: out.clone(),
            },
            out,
        )
    }

    fn bindargs(
        &mut self,
        stem: &str,
        sig: (&[syntax::Parameter], &[syntax::Generic]),
        args: &[syntax::Expression],
        subst: &mut Map,
    ) -> Bound {
        let (inputs, generics) = sig;
        let (mut lowered, mut constants, mut suffix) = (Vec::new(), BTreeMap::new(), Vec::new());
        for (param, arg) in inputs.iter().zip(args) {
            if param.constant {
                let mut eval = self.engine.evaluator(&self.stem, self.subst.clone());
                eval.vars.extend(self.constants.clone());
                let val = eval.eval(arg);
                if let Value::Int(n, _) = &val {
                    suffix.push(n.to_string());
                }
                constants.insert(param.name.clone(), val);
                continue;
            }
            let expected = param
                .annotation
                .as_ref()
                .filter(|s| !crate::flow::method::parametric(s, generics))
                .map(|s| self.engine.typing.resolve(stem, s, subst));
            let (val, ak) = self.expr(arg, expected.as_ref());
            if let Some(spec) = &param.annotation {
                crate::flow::method::unify((&self.engine.typing, spec), &ak, generics, subst);
            }
            lowered.push((val, ak, param.annotation.clone()));
        }
        Bound {
            lowered,
            constants,
            suffix,
        }
    }

    fn coerceargs(&mut self, stem: &str, lowered: Prepared, subst: &Map) -> Vec<Expression> {
        let mut out = Vec::new();
        for (val, ak, spec) in lowered {
            let want = spec
                .as_ref()
                .map(|s| self.engine.typing.resolve(stem, s, subst))
                .unwrap_or_else(|| ak.clone());
            out.push(self.coerce(val, &ak, &want));
        }
        out
    }
}

fn matched(payloads: &[syntax::Spec], target: &str, lowered: &[(Expression, Type)]) -> Type {
    payloads
        .iter()
        .enumerate()
        .find_map(|(i, s)| match s {
            syntax::Spec::Named {
                module: None, name, ..
            } if name == target => lowered.get(i).map(|(_, t)| t.clone()),
            _ => None,
        })
        .unwrap_or(Type::Unsigned(8))
}
