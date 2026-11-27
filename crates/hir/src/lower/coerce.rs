use crate::front::numeric::{encode, narrow};
use crate::front::world::Claim;
use crate::layout::Type;
use crate::lower::frame::Frame;
use crate::tree::Expression;
use simplex::tree as syntax;

impl<'b, 'a> Frame<'b, 'a> {
    pub fn coerce(&mut self, expr: Expression, actual: &Type, expected: &Type) -> Expression {
        if actual == expected || matches!(expected, Type::Void) {
            return expr;
        }
        if let (Expression::Int(n, _), Type::Signed(_) | Type::Unsigned(_)) = (&expr, expected) {
            return Expression::Int(narrow(*n, expected), expected.clone());
        }
        if let (Type::Pointer(inner), Type::Slice(_)) = (actual, expected)
            && matches!(inner.as_ref(), Type::List(_) | Type::Slice(_))
        {
            let target = match expr {
                Expression::Borrow { target, .. } => *target,
                other => Expression::Load {
                    pointer: Box::new(other),
                    kind: *inner.clone(),
                },
            };
            return slice(target, expected);
        }
        if let (Type::List(_), Type::Slice(_)) = (actual, expected) {
            return slice(expr, expected);
        }
        if let (Type::Pointer(inner), Type::Trait(contract)) = (actual, expected) {
            let raw = encode(inner);
            let owner = self
                .engine
                .typing
                .owner(&raw)
                .cloned()
                .unwrap_or_else(|| (self.stem.clone(), raw, Vec::new()));
            if let Some(claim) = self.conformance(&owner.0, &owner.1, contract) {
                let tsym = self.engine.table(&claim, &owner.2);
                return Expression::Coerce {
                    value: Box::new(expr),
                    table: tsym,
                    kind: expected.clone(),
                };
            }
        }
        expr
    }

    fn conformance(&self, owner: &str, subject: &str, contract: &str) -> Option<Claim> {
        let world = self.engine.world;
        let matches = |c: &Claim| {
            c.subject == subject
                && (world.traitname(&c.target.0, &c.target.1) == contract || c.target.1 == contract)
        };
        world
            .claims
            .iter()
            .find(|c| c.module == owner && matches(c))
            .or_else(|| world.claims.iter().find(|c| matches(c)))
            .cloned()
    }

    pub fn window(
        &mut self,
        base: &syntax::Expression,
        start: &syntax::Expression,
        end: &syntax::Expression,
    ) -> (Expression, Type) {
        let (raw, bkind) = self.place(base);
        let (src, peeled) = load(raw, bkind);
        let elem = match peeled {
            Type::Slice(e) | Type::List(e) => *e,
            _ => Type::Unsigned(1),
        };
        let skind = Type::Slice(Box::new(elem));
        let (lo, lk) = self.expr(start, Some(&Type::Unsigned(8)));
        let (hi, hk) = self.expr(end, Some(&Type::Unsigned(8)));
        (
            Expression::Window {
                base: Box::new(src),
                start: Box::new(self.coerce(lo, &lk, &Type::Unsigned(8))),
                limit: Box::new(self.coerce(hi, &hk, &Type::Unsigned(8))),
                kind: skind.clone(),
            },
            skind,
        )
    }

    pub fn indexed(
        &mut self,
        base: &syntax::Expression,
        start: &syntax::Expression,
        limit: Option<&syntax::Expression>,
    ) -> (Expression, Type) {
        if let Some(end) = limit {
            return self.window(base, start, end);
        }
        let (raw, bkind) = self.place(base);
        let (src, peeled) = load(raw, bkind);
        let elem = match peeled {
            Type::Slice(e) | Type::List(e) => *e,
            _ => Type::Unsigned(8),
        };
        let (idx, ik) = self.expr(start, Some(&Type::Unsigned(8)));
        (
            Expression::Index {
                base: Box::new(src),
                index: Box::new(self.coerce(idx, &ik, &Type::Unsigned(8))),
                kind: elem.clone(),
            },
            elem,
        )
    }

    pub fn provenance(
        &mut self,
        target: (&str, &str),
        pos: (usize, usize),
        runtime: &mut Vec<Expression>,
    ) {
        let callers = &self.engine.world.callers;
        if !callers.contains(&(target.0.to_owned(), target.1.to_owned())) {
            return;
        }
        self.engine.typing.origin();
        let (ns, module) = self.engine.world.coords(&self.engine.ns, &self.stem);
        let (ns, module) = (ns.to_owned(), module.to_owned());
        let files = &self.engine.world.files;
        let file = files
            .get(&self.stem)
            .cloned()
            .unwrap_or_else(|| format!("{module}.sx"));
        let origin = self.engine.pool.origin((&ns, &module, &file), pos);
        runtime.push(origin);
    }
}

pub fn load(expr: Expression, kind: Type) -> (Expression, Type) {
    match kind {
        Type::Pointer(inner) => (
            Expression::Load {
                pointer: Box::new(expr),
                kind: *inner.clone(),
            },
            *inner,
        ),
        other => (expr, other),
    }
}

fn slice(base: Expression, kind: &Type) -> Expression {
    Expression::Window {
        base: Box::new(base.clone()),
        start: Box::new(Expression::Int(0, Type::Unsigned(8))),
        limit: Box::new(Expression::Length(Box::new(base))),
        kind: kind.clone(),
    }
}
