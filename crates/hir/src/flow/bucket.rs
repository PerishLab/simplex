use crate::eval::asset::zero;
use crate::front::numeric::narrow;
use crate::layout::Type;
use crate::lower::frame::Frame;
use crate::tree::{Expression, Statement};
use simplex::tree as syntax;

impl<'b, 'a> Frame<'b, 'a> {
    pub fn bucket(&mut self, p: &syntax::Partition, expected: Option<&Type>) -> (Expression, Type) {
        let mut prelude = Vec::new();
        let sub = p.subject.as_ref().map(|s| {
            let (val, skind) = self.expr(s, None);
            let tmp = self.engine.fresh("sub");
            prelude.push(Statement::Bind {
                name: tmp.clone(),
                mutable: false,
                kind: skind.clone(),
                value: val,
            });
            (tmp, skind)
        });
        let (mut pairs, mut fallback) = (Vec::new(), None);
        let mut chosen = expected.cloned().unwrap_or(Type::Void);
        let mut locked = !matches!(chosen, Type::Void);
        for branch in &p.arms {
            let hint = locked.then_some(&chosen).or(expected);
            if matches!(branch.pattern, syntax::Pattern::Wildcard) {
                let (out, okind) = self.outcome(&branch.outcome, hint);
                if !locked && !matches!(okind, Type::Void) {
                    chosen = okind;
                    locked = !literal(&branch.outcome);
                }
                fallback = Some(out);
            } else {
                let cond = self.condition(sub.as_ref(), &branch.pattern);
                let (out, okind) = self.outcome(&branch.outcome, hint);
                if !locked && !matches!(okind, Type::Void) {
                    chosen = okind;
                    locked = !literal(&branch.outcome);
                }
                pairs.push((cond, out));
            }
        }
        let catalog = &self.engine.typing.catalog;
        let mut acc = retype(fallback.unwrap_or_else(|| zero(&chosen, catalog)), &chosen);
        for (cond, out) in pairs.into_iter().rev() {
            acc = Expression::Branch {
                condition: Box::new(cond),
                then: Box::new(retype(out, &chosen)),
                otherwise: Box::new(acc),
                kind: chosen.clone(),
            };
        }
        match prelude.is_empty() {
            true => (acc, chosen),
            false => (
                Expression::Block {
                    body: prelude,
                    tail: Some(Box::new(acc)),
                    kind: chosen.clone(),
                },
                chosen,
            ),
        }
    }

    fn condition(&mut self, sub: Option<&(String, Type)>, pat: &syntax::Pattern) -> Expression {
        match pat {
            syntax::Pattern::Wildcard => Expression::Bool(true),
            syntax::Pattern::Predicate(e) => self.expr(e, Some(&Type::Bool)).0,
            syntax::Pattern::Range { start, end } => self.ranged(sub, (start, end)),
            syntax::Pattern::Choices(list) => self.choices(sub, list),
        }
    }

    fn ranged(
        &mut self,
        sub: Option<&(String, Type)>,
        bounds: (&syntax::Expression, &syntax::Expression),
    ) -> Expression {
        let Some((name, kind)) = sub else {
            return Expression::Bool(false);
        };
        let lhs = Expression::Local(name.clone(), kind.clone());
        let (lo, lk) = self.expr(bounds.0, Some(kind));
        let (hi, hk) = self.expr(bounds.1, Some(kind));
        let ge = Expression::Binary {
            operator: String::from(">="),
            left: Box::new(lhs.clone()),
            right: Box::new(self.coerce(lo, &lk, kind)),
            kind: Type::Bool,
        };
        let lt = Expression::Binary {
            operator: String::from("<"),
            left: Box::new(lhs),
            right: Box::new(self.coerce(hi, &hk, kind)),
            kind: Type::Bool,
        };
        Expression::Binary {
            operator: String::from("&&"),
            left: Box::new(ge),
            right: Box::new(lt),
            kind: Type::Bool,
        }
    }

    fn choices(&mut self, sub: Option<&(String, Type)>, list: &[syntax::Expression]) -> Expression {
        let Some((name, kind)) = sub else {
            return Expression::Bool(false);
        };
        let mut iter = list.iter();
        let Some(first) = iter.next() else {
            return Expression::Bool(false);
        };
        let mut acc = self.equals(name, kind, first);
        for rest in iter {
            let next = self.equals(name, kind, rest);
            acc = Expression::Binary {
                operator: String::from("||"),
                left: Box::new(acc),
                right: Box::new(next),
                kind: Type::Bool,
            };
        }
        acc
    }

    fn equals(&mut self, name: &str, kind: &Type, pat: &syntax::Expression) -> Expression {
        let (rhs, rk) = self.expr(pat, Some(kind));
        Expression::Binary {
            operator: String::from("=="),
            left: Box::new(Expression::Local(name.to_owned(), kind.clone())),
            right: Box::new(self.coerce(rhs, &rk, kind)),
            kind: Type::Bool,
        }
    }
}

pub fn literal(out: &syntax::Outcome) -> bool {
    matches!(out, syntax::Outcome::Value(syntax::Expression::Number(t)) if !t.contains('.'))
}

pub fn retype(expr: Expression, kind: &Type) -> Expression {
    match (expr, kind) {
        (Expression::Int(n, _), Type::Signed(_) | Type::Unsigned(_)) => {
            Expression::Int(narrow(n, kind), kind.clone())
        }
        (other, _) => other,
    }
}
