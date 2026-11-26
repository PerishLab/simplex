use crate::eval::asset::zero;
use crate::flow::fallible::qualified;
use crate::layout::Type;
use crate::lower::coerce::load;
use crate::lower::frame::Frame;
use crate::tree::Expression;
use simplex::tree as syntax;
use std::collections::BTreeMap;

impl<'b, 'a> Frame<'b, 'a> {
    pub fn member(
        &mut self,
        target: (&syntax::Expression, &str),
        generics: &[syntax::Spec],
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let (base, field) = target;
        if field == "await" {
            return self.awaited(base);
        }
        if let Some(unit) = self.unit(base, field, expected) {
            return unit;
        }
        self.projection(base, field, generics)
    }

    fn awaited(&mut self, base: &syntax::Expression) -> (Expression, Type) {
        if let syntax::Expression::Call {
            callee, arguments, ..
        } = base
            && arguments.is_empty()
            && let syntax::Expression::Member {
                base: list,
                field: op,
                ..
            } = callee.as_ref()
            && matches!(op.as_str(), "all" | "race")
        {
            return self.joined(list, op);
        }
        let (fiber, fkind) = self.expr(base, None);
        let out = match fkind {
            Type::Fiber(inner) => *inner,
            other => other,
        };
        (
            Expression::Await {
                fiber: Box::new(fiber),
                kind: out.clone(),
            },
            out,
        )
    }

    fn joined(&mut self, list: &syntax::Expression, op: &str) -> (Expression, Type) {
        let (fibers, lkind) = self.expr(list, None);
        let elem = match lkind {
            Type::List(inner) => match *inner {
                Type::Fiber(t) => *t,
                other => other,
            },
            _ => Type::Unsigned(8),
        };
        if op == "all" {
            let out = Type::List(Box::new(elem.clone()));
            return (
                Expression::All {
                    fibers: Box::new(fibers),
                    element: elem,
                    kind: out.clone(),
                },
                out,
            );
        }
        (
            Expression::Race {
                fibers: Box::new(fibers),
                kind: elem.clone(),
            },
            elem,
        )
    }

    pub fn projection(
        &mut self,
        base: &syntax::Expression,
        field: &str,
        generics: &[syntax::Spec],
    ) -> (Expression, Type) {
        let _ = generics;
        let (raw, bkind) = self.place(base);
        let (src, peeled) = load(raw, bkind);
        if let Type::Record(name) = &peeled {
            let catalog = &self.engine.typing.catalog;
            if let Some(slot) = catalog
                .records
                .get(name)
                .and_then(|r| r.fields.iter().find(|f| f.name == field))
                .cloned()
            {
                return (
                    Expression::Field {
                        base: Box::new(src),
                        name: slot.name,
                        offset: slot.offset,
                        kind: slot.kind.clone(),
                    },
                    slot.kind,
                );
            }
        }
        (
            Expression::Field {
                base: Box::new(src),
                name: field.to_owned(),
                offset: 0,
                kind: Type::Unsigned(8),
            },
            Type::Unsigned(8),
        )
    }

    pub fn unit(
        &mut self,
        base: &syntax::Expression,
        arm: &str,
        expected: Option<&Type>,
    ) -> Option<(Expression, Type)> {
        let (owner, name) = qualified(base, &self.stem, &self.engine.world.imports)?;
        let kind = match expected {
            Some(Type::Choice(k)) => Type::Choice(k.clone()),
            _ => self.engine.typing.nominal(&owner, &name, &[]),
        };
        let Type::Choice(key) = &kind else {
            return None;
        };
        let catalog = &self.engine.typing.catalog;
        let tag = catalog
            .choices
            .get(key)
            .and_then(|c| c.arms.iter().find(|a| a.name == arm))
            .map_or(0, |a| a.tag);
        Some((
            Expression::Variant {
                choice: key.clone(),
                arm: arm.to_owned(),
                tag,
                payloads: Vec::new(),
                kind: kind.clone(),
            },
            kind,
        ))
    }

    pub fn record(
        &mut self,
        owner: (Option<&str>, &str),
        fields: &[syntax::Field],
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let module = owner.0.map_or_else(
            || self.stem.clone(),
            |m| self.engine.world.target(&self.stem, m),
        );
        let (_, resolved) = self.engine.world.unalias(&module, owner.1);
        let raw = match &resolved {
            syntax::Spec::Named { name, .. } => name.clone(),
            _ => owner.1.to_owned(),
        };
        let mut lowered = BTreeMap::new();
        for f in fields {
            lowered.insert(f.name.clone(), self.expr(&f.value, None));
        }
        let kind = match expected {
            Some(Type::Record(k)) => Type::Record(k.clone()),
            _ => self.infer(&module, &raw, &lowered),
        };
        let key = match &kind {
            Type::Record(k) => k.clone(),
            _ => raw,
        };
        let ordered = self.ordered(&key, lowered);
        (
            Expression::Record {
                name: key,
                fields: ordered,
                kind: kind.clone(),
            },
            kind,
        )
    }

    fn infer(
        &mut self,
        module: &str,
        raw: &str,
        lowered: &BTreeMap<String, (Expression, Type)>,
    ) -> Type {
        let structs = &self.engine.world.structs;
        let Some(def) = structs.get(&(module.to_owned(), raw.to_owned())).cloned() else {
            return self.engine.typing.nominal(module, raw, &[]);
        };
        let gargs: Vec<Type> = def
            .generics
            .iter()
            .map(|g| {
                def.fields
                    .iter()
                    .find(|s| matches!(&s.spec, syntax::Spec::Named { module: None, name, .. } if name == &g.name))
                    .and_then(|s| lowered.get(&s.name).map(|(_, t)| t.clone()))
                    .unwrap_or(Type::Unsigned(8))
            })
            .collect();
        self.engine.typing.nominal(module, raw, &gargs)
    }

    fn ordered(
        &mut self,
        key: &str,
        mut lowered: BTreeMap<String, (Expression, Type)>,
    ) -> Vec<(String, Expression)> {
        let layout = self.engine.typing.catalog.records.get(key).cloned();
        let Some(def) = layout else {
            return lowered.into_iter().map(|(k, (v, _))| (k, v)).collect();
        };
        let mut out = Vec::new();
        for slot in &def.fields {
            let expr = match lowered.remove(&slot.name) {
                Some((v, vt)) => self.coerce(v, &vt, &slot.kind),
                None => zero(&slot.kind, &self.engine.typing.catalog),
            };
            out.push((slot.name.clone(), expr));
        }
        out
    }

    pub fn listed(
        &mut self,
        items: &[syntax::Expression],
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let hint = match expected {
            Some(Type::List(e) | Type::Slice(e)) => Some(e.as_ref()),
            _ => None,
        };
        let mut out = Vec::new();
        let mut elem = hint.cloned().unwrap_or(Type::Void);
        for item in items {
            let (val, ik) = self.expr(item, (!matches!(elem, Type::Void)).then_some(&elem));
            if matches!(elem, Type::Void) {
                elem = ik.clone();
            }
            out.push(self.coerce(val, &ik, &elem));
        }
        if matches!(elem, Type::Void) {
            elem = Type::Unsigned(8);
        }
        let lkind = Type::List(Box::new(elem.clone()));
        (
            Expression::List {
                items: out,
                element: elem,
                kind: lkind.clone(),
            },
            lkind,
        )
    }
}
