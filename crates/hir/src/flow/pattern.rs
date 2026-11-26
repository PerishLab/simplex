use crate::eval::asset::zero;
use crate::flow::bucket::{literal, retype};
use crate::layout::{Field, Tagged, Type};
use crate::lower::coerce::load;
use crate::lower::frame::Frame;
use crate::tree::{Case, Expression, Statement};
use simplex::tree as syntax;

struct Staged {
    tags: Vec<u64>,
    bindings: Vec<(String, Field)>,
    raw: Expression,
    drops: Vec<Statement>,
}

impl<'b, 'a> Frame<'b, 'a> {
    pub fn switch(&mut self, d: &syntax::Dispatch, expected: Option<&Type>) -> (Expression, Type) {
        let (sub, skind) = self.expr(&d.subject, None);
        let (val, inner) = load(sub, skind);
        if matches!(inner, Type::Bool) {
            return self.boolean(val, &d.arms, expected);
        }
        self.tagged((val, &inner), &d.arms, expected)
    }

    fn boolean(
        &mut self,
        cond: Expression,
        arms: &[syntax::Branch],
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let (mut truthy, mut falsy) = (None, None);
        let mut chosen = expected.cloned().unwrap_or(Type::Void);
        let mut locked = !matches!(chosen, Type::Void);
        for branch in arms {
            let hint = locked.then_some(&chosen).or(expected);
            let (out, okind) = self.outcome(&branch.outcome, hint);
            if !locked && !matches!(okind, Type::Void) {
                chosen = okind;
                locked = !literal(&branch.outcome);
            }
            assign(&branch.pattern, &out, &mut truthy, &mut falsy);
        }
        let catalog = &self.engine.typing.catalog;
        let then = retype(truthy.unwrap_or_else(|| zero(&chosen, catalog)), &chosen);
        let otherwise = retype(falsy.unwrap_or_else(|| zero(&chosen, catalog)), &chosen);
        (
            Expression::Branch {
                condition: Box::new(cond),
                then: Box::new(then),
                otherwise: Box::new(otherwise),
                kind: chosen.clone(),
            },
            chosen,
        )
    }

    fn tagged(
        &mut self,
        scrutinee: (Expression, &Type),
        arms: &[syntax::Branch],
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let (subject, skind) = scrutinee;
        let choice = match skind {
            Type::Choice(c) => c.clone(),
            _ => String::new(),
        };
        let catalog = &self.engine.typing.catalog;
        let def = catalog.choices.get(&choice).cloned();
        let mut staged = Vec::new();
        let mut chosen = expected.cloned().unwrap_or(Type::Void);
        let mut locked = !matches!(chosen, Type::Void);
        for branch in arms {
            let (tags, bindings) = extract(&branch.pattern, def.as_ref());
            self.push();
            for (bname, bfield) in &bindings {
                self.declare(bname, bfield.kind.clone());
            }
            let hint = locked.then_some(&chosen).or(expected);
            let (raw, okind) = self.outcome(&branch.outcome, hint);
            if !locked && !matches!(okind, Type::Void) {
                chosen = okind;
                locked = !literal(&branch.outcome);
            }
            let mut drops = Vec::new();
            self.pop(&mut drops);
            staged.push(Staged {
                tags,
                bindings,
                raw,
                drops,
            });
        }
        let cases = staged
            .into_iter()
            .map(|item| Case {
                tags: item.tags,
                bindings: item.bindings,
                body: wrap(retype(item.raw, &chosen), item.drops, &chosen),
            })
            .collect();
        (
            Expression::Switch {
                subject: Box::new(subject),
                cases,
                kind: chosen.clone(),
            },
            chosen,
        )
    }

    pub fn outcome(
        &mut self,
        out: &syntax::Outcome,
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        match out {
            syntax::Outcome::Value(e) => self.expr(e, expected),
            syntax::Outcome::Block(b) => self.scoped(b, expected),
            syntax::Outcome::Break(opt) => {
                let target = self.targets.last().and_then(Clone::clone);
                let val = opt.as_ref().map(|e| {
                    let (raw, actual) = self.expr(e, target.as_ref());
                    let want = target.clone().unwrap_or_else(|| actual.clone());
                    (self.coerce(raw, &actual, &want), want)
                });
                let floor = self.loops.last().copied().unwrap_or(0);
                let mut stmts = self.unwind(floor);
                let kind = val.as_ref().map_or(Type::Void, |(_, k)| k.clone());
                if !matches!(kind, Type::Void)
                    && let Some(slot) = self.breaks.last_mut()
                {
                    *slot = Some(kind);
                }
                stmts.push(Statement::Break(val.map(|(v, _)| v)));
                (
                    Expression::Block {
                        body: stmts,
                        tail: None,
                        kind: Type::Void,
                    },
                    Type::Void,
                )
            }
            syntax::Outcome::Continue => {
                let floor = self.loops.last().copied().unwrap_or(0);
                let mut stmts = self.unwind(floor);
                stmts.push(Statement::Continue);
                (
                    Expression::Block {
                        body: stmts,
                        tail: None,
                        kind: Type::Void,
                    },
                    Type::Void,
                )
            }
        }
    }

    pub fn repetition(
        &mut self,
        kind: &syntax::Loop,
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let target = expected.cloned().filter(|t| !matches!(t, Type::Void));
        self.loops.push(self.scopes.len());
        self.breaks.push(None);
        self.targets.push(target.clone());
        let mut steps = Vec::new();
        match kind {
            syntax::Loop::Block(b) => steps.push(Statement::Evaluate(self.scoped(b, None).0)),
            syntax::Loop::Switch(d) => {
                steps.push(Statement::Evaluate(self.switch(d, Some(&Type::Void)).0));
            }
            syntax::Loop::Bucket(p) => {
                steps.push(Statement::Evaluate(self.bucket(p, Some(&Type::Void)).0));
            }
        }
        self.loops.pop();
        self.targets.pop();
        let yielded = self.breaks.pop().flatten();
        let out = target.or(yielded).unwrap_or(Type::Void);
        (
            Expression::Loop {
                body: steps,
                kind: out.clone(),
            },
            out,
        )
    }
}

fn assign(
    pat: &syntax::Pattern,
    out: &Expression,
    truthy: &mut Option<Expression>,
    falsy: &mut Option<Expression>,
) {
    let syntax::Pattern::Choices(list) = pat else {
        return;
    };
    for item in list {
        match matches!(item, syntax::Expression::Word { text, .. } if text == "true") {
            true => *truthy = Some(out.clone()),
            false => *falsy = Some(out.clone()),
        }
    }
}

fn extract(pat: &syntax::Pattern, def: Option<&Tagged>) -> (Vec<u64>, Vec<(String, Field)>) {
    let (mut tags, mut bindings) = (Vec::new(), Vec::new());
    let syntax::Pattern::Choices(list) = pat else {
        return (tags, bindings);
    };
    for expr in list {
        arm(expr, def, &mut tags, &mut bindings);
    }
    (tags, bindings)
}

fn arm(
    expr: &syntax::Expression,
    def: Option<&Tagged>,
    tags: &mut Vec<u64>,
    bindings: &mut Vec<(String, Field)>,
) {
    match expr {
        syntax::Expression::Member { field, .. } => {
            let tag = def
                .and_then(|t| t.arms.iter().find(|a| a.name == *field))
                .map_or(0, |a| a.tag);
            tags.push(tag);
        }
        syntax::Expression::Call {
            callee, arguments, ..
        } => payload(callee, arguments, def, (tags, bindings)),
        _ => {}
    }
}

fn payload(
    callee: &syntax::Expression,
    arguments: &[syntax::Expression],
    def: Option<&Tagged>,
    out: (&mut Vec<u64>, &mut Vec<(String, Field)>),
) {
    let syntax::Expression::Member { field, .. } = callee else {
        return;
    };
    let Some(found) = def.and_then(|t| t.arms.iter().find(|a| a.name == *field)) else {
        return;
    };
    out.0.push(found.tag);
    for (arg, slot) in arguments.iter().zip(&found.payloads) {
        if let syntax::Expression::Word { text, .. } = arg {
            out.1.push((text.clone(), slot.clone()));
        }
    }
}

fn wrap(expr: Expression, mut drops: Vec<Statement>, kind: &Type) -> Expression {
    if drops.is_empty() {
        return expr;
    }
    if matches!(kind, Type::Void) {
        let mut body = vec![Statement::Evaluate(expr)];
        body.append(&mut drops);
        return Expression::Block {
            body,
            tail: None,
            kind: Type::Void,
        };
    }
    let name = String::from("$arm");
    let mut body = vec![Statement::Bind {
        name: name.clone(),
        mutable: false,
        kind: kind.clone(),
        value: expr,
    }];
    body.append(&mut drops);
    Expression::Block {
        body,
        tail: Some(Box::new(Expression::Local(name, kind.clone()))),
        kind: kind.clone(),
    }
}
