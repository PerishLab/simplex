use crate::front::numeric::encode;
use crate::front::types::{Map, Typing};
use crate::layout::{Mangle, Type};
use crate::lower::coerce::load;
use crate::lower::frame::Frame;
use crate::tree::Expression;
use simplex::tree as syntax;

impl<'b, 'a> Frame<'b, 'a> {
    pub fn route(
        &mut self,
        base: &syntax::Expression,
        field: &str,
        call: (&[syntax::Spec], &[syntax::Expression]),
        pos: (usize, usize),
    ) -> (Expression, Type) {
        if let syntax::Expression::Word { text, .. } = base
            && self.lookup(text).is_none()
            && let Some((leaf, linked)) = self
                .engine
                .world
                .imports
                .get(&(self.stem.clone(), text.clone()))
                .cloned()
        {
            return self.modulecall((&leaf, linked), field, call, pos);
        }
        self.method(base, field, call.1)
    }

    fn modulecall(
        &mut self,
        target: (&str, bool),
        field: &str,
        call: (&[syntax::Spec], &[syntax::Expression]),
        pos: (usize, usize),
    ) -> (Expression, Type) {
        let (leaf, linked) = target;
        if linked {
            let sym = self.engine.symbol(leaf, field);
            if let Some(imp) = self.engine.imports.get(&sym).cloned() {
                let args = self.arguments(call.1, &imp.inputs);
                return (
                    Expression::Call {
                        symbol: sym,
                        arguments: args,
                        kind: imp.output.clone(),
                    },
                    imp.output,
                );
            }
        }
        self.direct(leaf, field, call, pos)
    }

    fn method(
        &mut self,
        base: &syntax::Expression,
        field: &str,
        args: &[syntax::Expression],
    ) -> (Expression, Type) {
        let (recv, rkind) = self.place(base);
        let (peeled, ptype) = load(recv.clone(), rkind.clone());
        if let Type::Trait(cname) = &ptype {
            return self.dynamic(peeled, cname, field, args);
        }
        let raw = encode(&ptype);
        let (owner, sname, gargs) = self
            .engine
            .typing
            .owner(&raw)
            .cloned()
            .unwrap_or_else(|| (self.stem.clone(), raw, Vec::new()));
        let Some(claim) = self.locate(&owner, &sname, field) else {
            let mut full = vec![recv];
            full.extend(args.iter().map(|a| self.expr(a, None).0));
            let sym = self.engine.symbol(&owner, field);
            return (
                Expression::Call {
                    symbol: sym,
                    arguments: full,
                    kind: Type::Unsigned(8),
                },
                Type::Unsigned(8),
            );
        };
        let tsym = self.engine.table(&claim, &gargs);
        let msym = Mangle::method(&tsym, field);
        let info = self
            .engine
            .routines
            .iter()
            .find(|r| r.symbol == msym)
            .map(|r| (r.inputs.clone(), r.output.clone(), r.asynchronous));
        let (ins, out, asynchronous) = info.unwrap_or((Vec::new(), Type::Void, false));
        let receiver = self.receiver(base, recv, peeled, (&rkind, ins.first()));
        let expected: Vec<Type> = ins.iter().skip(1).map(|(_, t)| t.clone()).collect();
        let mut full = vec![receiver];
        full.extend(self.arguments(args, &expected));
        let ret = match asynchronous {
            true => Type::Fiber(Box::new(out)),
            false => out,
        };
        (
            Expression::Call {
                symbol: msym,
                arguments: full,
                kind: ret.clone(),
            },
            ret,
        )
    }

    fn locate(
        &self,
        owner: &str,
        subject: &str,
        field: &str,
    ) -> Option<crate::front::world::Claim> {
        let claims = &self.engine.world.claims;
        let interfaces = &self.engine.world.interfaces;
        let matches = |c: &crate::front::world::Claim| {
            c.subject == subject
                && (c.methods.contains_key(field)
                    || interfaces
                        .get(&c.target)
                        .is_some_and(|i| i.methods.iter().any(|m| m.name == field)))
        };
        claims
            .iter()
            .find(|c| c.module == owner && matches(c))
            .or_else(|| claims.iter().find(|c| matches(c)))
            .cloned()
    }

    fn receiver(
        &mut self,
        base: &syntax::Expression,
        recv: Expression,
        peeled: Expression,
        kinds: (&Type, Option<&(String, Type)>),
    ) -> Expression {
        match kinds.1 {
            Some((_, Type::Pointer(inner))) => match kinds.0 {
                Type::Pointer(_) => recv,
                _ => Expression::Borrow {
                    mutable: true,
                    target: Box::new(recv),
                    kind: Type::Pointer(inner.clone()),
                },
            },
            _ => {
                if let syntax::Expression::Word { text, .. } = base {
                    self.consume(text);
                }
                peeled
            }
        }
    }

    fn dynamic(
        &mut self,
        receiver: Expression,
        cname: &str,
        field: &str,
        args: &[syntax::Expression],
    ) -> (Expression, Type) {
        let world = self.engine.world;
        let found = world
            .interfaces
            .iter()
            .find(|((m, n), _)| world.traitname(m, n) == cname || n == cname)
            .map(|((m, _), c)| (m.clone(), c.clone()));
        let Some((imod, contract)) = found else {
            let lowered = args.iter().map(|a| self.expr(a, None).0).collect();
            return (
                Expression::Virtual {
                    receiver: Box::new(receiver),
                    slot: 0,
                    arguments: lowered,
                    kind: Type::Unsigned(8),
                },
                Type::Unsigned(8),
            );
        };
        let slot = contract
            .methods
            .iter()
            .position(|m| m.name == field)
            .unwrap_or(0);
        let req = &contract.methods[slot];
        let mut subst = Map::new();
        subst.insert(String::from("Self"), Type::Trait(cname.to_owned()));
        let expected: Vec<Type> = req
            .inputs
            .iter()
            .skip(1)
            .map(|p| {
                self.engine
                    .typing
                    .optional(&imod, p.annotation.as_ref(), &subst)
            })
            .collect();
        let lowered = self.arguments(args, &expected);
        let raw = self
            .engine
            .typing
            .optional(&imod, req.output.as_ref(), &subst);
        let out = match req.asynchronous {
            true => Type::Fiber(Box::new(raw)),
            false => raw,
        };
        (
            Expression::Virtual {
                receiver: Box::new(receiver),
                slot,
                arguments: lowered,
                kind: out.clone(),
            },
            out,
        )
    }
}

pub fn parametric(spec: &syntax::Spec, generics: &[syntax::Generic]) -> bool {
    match spec {
        syntax::Spec::Named {
            module,
            name,
            arguments,
        } => {
            (module.is_none() && generics.iter().any(|g| g.name == *name))
                || arguments.iter().any(|a| parametric(a, generics))
        }
        syntax::Spec::Borrow { target, .. } | syntax::Spec::Slice(target) => {
            parametric(target, generics)
        }
        _ => false,
    }
}

pub fn unify(
    ctx: (&Typing<'_>, &syntax::Spec),
    actual: &Type,
    generics: &[syntax::Generic],
    subst: &mut Map,
) {
    let (typing, spec) = ctx;
    match (spec, actual) {
        (
            syntax::Spec::Named {
                module: None,
                name,
                arguments,
            },
            _,
        ) if arguments.is_empty() && generics.iter().any(|g| g.name == *name) => {
            subst.entry(name.clone()).or_insert_with(|| actual.clone());
        }
        (syntax::Spec::Named { arguments, .. }, Type::Record(key) | Type::Choice(key))
            if !arguments.is_empty() =>
        {
            unifyrecord((typing, arguments), key, generics, subst)
        }
        (
            syntax::Spec::Named { arguments, .. },
            Type::List(inner) | Type::Fiber(inner) | Type::Slice(inner),
        ) if arguments.len() == 1 => unify((typing, &arguments[0]), inner, generics, subst),
        (syntax::Spec::Borrow { target, .. }, Type::Pointer(inner)) => {
            unify((typing, target), inner, generics, subst);
        }
        (syntax::Spec::Slice(elem), Type::Slice(inner)) => {
            unify((typing, elem), inner, generics, subst);
        }
        _ => {}
    }
}

fn unifyrecord(
    ctx: (&Typing<'_>, &[syntax::Spec]),
    key: &str,
    generics: &[syntax::Generic],
    subst: &mut Map,
) {
    let (typing, arguments) = ctx;
    let Some((_, _, args)) = typing.owner(key) else {
        return;
    };
    for (sub, found) in arguments.iter().zip(args) {
        unify((typing, sub), found, generics, subst);
    }
}
