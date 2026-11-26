use crate::layout::Type;
use crate::lower::frame::{Frame, Unrolled};
use crate::tree::{Expression, Statement};
use simplex::tree as syntax;

impl<'b, 'a> Frame<'b, 'a> {
    pub fn bind(&mut self, b: &syntax::Binding, out: &mut Vec<Statement>) {
        let annotated = b
            .annotation
            .as_ref()
            .map(|s| self.engine.typing.resolve(&self.stem, s, &self.subst));
        let (raw, actual) = self.expr(&b.value, annotated.as_ref());
        let kind = annotated.unwrap_or_else(|| actual.clone());
        let value = self.coerce(raw, &actual, &kind);
        out.push(Statement::Bind {
            name: b.name.clone(),
            mutable: b.mutable,
            kind: kind.clone(),
            value,
        });
        self.declare(&b.name, kind);
    }

    pub fn assign(
        &mut self,
        triple: (&syntax::Expression, &str, &syntax::Expression),
        out: &mut Vec<Statement>,
    ) {
        let (target, operator, value) = triple;
        if let syntax::Expression::Word { text, .. } = target {
            let kind = self.lookup(text).unwrap_or(Type::Signed(8));
            let (raw, actual) = self.expr(value, Some(&kind));
            let rhs = self.coerce(raw, &actual, &kind);
            if operator == "=" {
                self.revive(text);
                out.push(Statement::Assign {
                    name: text.clone(),
                    value: rhs,
                });
                return;
            }
            let op = operator.trim_end_matches('=').to_owned();
            out.push(Statement::Assign {
                name: text.clone(),
                value: Expression::Binary {
                    operator: op,
                    left: Box::new(Expression::Local(text.clone(), kind.clone())),
                    right: Box::new(rhs),
                    kind,
                },
            });
            return;
        }
        let (lhs, lkind) = self.place(target);
        let (raw, actual) = self.expr(value, Some(&lkind));
        let rhs = self.coerce(raw, &actual, &lkind);
        let next = match operator {
            "=" => rhs,
            _ => Expression::Binary {
                operator: operator.trim_end_matches('=').to_owned(),
                left: Box::new(lhs.clone()),
                right: Box::new(rhs),
                kind: lkind,
            },
        };
        out.push(Statement::Store {
            target: lhs,
            value: next,
        });
    }

    pub fn unroll(
        &mut self,
        triple: (&str, &syntax::Expression, &[syntax::Item]),
        out: &mut Vec<Statement>,
    ) {
        let (name, source, items) = triple;
        let syntax::Expression::Call { generics, .. } = source else {
            return;
        };
        let Some(spec) = generics.first() else {
            return;
        };
        let resolved = self.engine.typing.resolve(&self.stem, spec, &self.subst);
        let Type::Record(key) = resolved else {
            return;
        };
        let Some((owner, raw, _)) = self.engine.typing.owner(&key).cloned() else {
            return;
        };
        let structs = &self.engine.world.structs;
        let Some(def) = structs.get(&(owner, raw)).cloned() else {
            return;
        };
        for slot in &def.fields {
            let tags = slot.decorators.iter().map(|d| d.name.clone()).collect();
            self.fields.insert(
                name.to_owned(),
                Unrolled {
                    name: slot.name.clone(),
                    tags,
                },
            );
            for item in items {
                self.item(item, out);
            }
        }
        self.fields.remove(name);
    }

    pub fn iterate(
        &mut self,
        triple: (&str, &syntax::Expression, &syntax::Block),
        out: &mut Vec<Statement>,
    ) {
        let (name, source, body) = triple;
        if let syntax::Expression::Binary {
            left,
            operator,
            right,
            ..
        } = source
            && operator == ".."
        {
            self.interval(name, (left, right), body, out);
            return;
        }
        self.sliced(name, source, body, out);
    }

    fn interval(
        &mut self,
        name: &str,
        bounds: (&syntax::Expression, &syntax::Expression),
        body: &syntax::Block,
        out: &mut Vec<Statement>,
    ) {
        let (lo, lk) = self.expr(bounds.0, None);
        let ikind = match lk {
            Type::Signed(_) | Type::Unsigned(_) => lk,
            _ => Type::Signed(8),
        };
        let (hi, hk) = self.expr(bounds.1, Some(&ikind));
        let limit = self.coerce(hi, &hk, &ikind);
        let idx = self.engine.fresh("idx");
        let end = self.engine.fresh("end");
        out.push(Statement::Bind {
            name: idx.clone(),
            mutable: true,
            kind: ikind.clone(),
            value: lo,
        });
        out.push(Statement::Bind {
            name: end.clone(),
            mutable: false,
            kind: ikind.clone(),
            value: limit,
        });
        let steps = self.stepped(
            name,
            &idx,
            (
                Expression::Local(end, ikind.clone()),
                Expression::Local(idx.clone(), ikind.clone()),
                ikind,
            ),
            body,
        );
        out.push(Statement::Evaluate(Expression::Loop {
            body: steps,
            kind: Type::Void,
        }));
    }

    fn sliced(
        &mut self,
        name: &str,
        source: &syntax::Expression,
        body: &syntax::Block,
        out: &mut Vec<Statement>,
    ) {
        let (raw, skind) = self.expr(source, None);
        let (src, container) = crate::lower::coerce::load(raw, skind);
        let elem = match &container {
            Type::Slice(e) | Type::List(e) => *e.clone(),
            _ => Type::Unsigned(8),
        };
        let holder = self.engine.fresh("src");
        let idx = self.engine.fresh("idx");
        let end = self.engine.fresh("len");
        out.push(Statement::Bind {
            name: holder.clone(),
            mutable: false,
            kind: container.clone(),
            value: src,
        });
        out.push(Statement::Bind {
            name: idx.clone(),
            mutable: true,
            kind: Type::Unsigned(8),
            value: Expression::Int(0, Type::Unsigned(8)),
        });
        out.push(Statement::Bind {
            name: end.clone(),
            mutable: false,
            kind: Type::Unsigned(8),
            value: Expression::Length(Box::new(Expression::Local(
                holder.clone(),
                container.clone(),
            ))),
        });
        let item = Expression::Index {
            base: Box::new(Expression::Local(holder, container)),
            index: Box::new(Expression::Local(idx.clone(), Type::Unsigned(8))),
            kind: elem.clone(),
        };
        let steps = self.stepped(
            name,
            &idx,
            (Expression::Local(end, Type::Unsigned(8)), item, elem),
            body,
        );
        out.push(Statement::Evaluate(Expression::Loop {
            body: steps,
            kind: Type::Void,
        }));
    }

    fn stepped(
        &mut self,
        name: &str,
        idx: &str,
        spec: (Expression, Expression, Type),
        body: &syntax::Block,
    ) -> Vec<Statement> {
        let (limit, current, elem) = spec;
        let ikind = match &limit {
            Expression::Local(_, k) => k.clone(),
            _ => Type::Unsigned(8),
        };
        let mut steps = vec![
            Statement::Evaluate(Expression::Branch {
                condition: Box::new(Expression::Binary {
                    operator: String::from(">="),
                    left: Box::new(Expression::Local(idx.to_owned(), ikind.clone())),
                    right: Box::new(limit),
                    kind: Type::Bool,
                }),
                then: Box::new(Expression::Block {
                    body: vec![Statement::Break(None)],
                    tail: None,
                    kind: Type::Void,
                }),
                otherwise: Box::new(Expression::Void),
                kind: Type::Void,
            }),
            Statement::Bind {
                name: name.to_owned(),
                mutable: false,
                kind: elem.clone(),
                value: current,
            },
            Statement::Assign {
                name: idx.to_owned(),
                value: Expression::Binary {
                    operator: String::from("+"),
                    left: Box::new(Expression::Local(idx.to_owned(), ikind.clone())),
                    right: Box::new(Expression::Int(1, ikind.clone())),
                    kind: ikind,
                },
            },
        ];
        self.push();
        self.loops.push(self.scopes.len() - 1);
        self.declare(name, elem);
        for sub in &body.items {
            self.item(sub, &mut steps);
        }
        self.loops.pop();
        self.pop(&mut steps);
        steps
    }
}
