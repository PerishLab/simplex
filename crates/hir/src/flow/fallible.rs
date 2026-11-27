use crate::layout::Type;
use crate::lower::coerce::load;
use crate::lower::engine::Spec;
use crate::lower::frame::Frame;
use crate::tree::{Case, Expression, Statement};
use simplex::tree as syntax;
use std::collections::BTreeMap;

struct Mapping {
    param: String,
    holder: String,
    count: String,
    buf: String,
    idx: String,
    peeled: Type,
    input: Type,
    output: Type,
    mapped: Expression,
    drops: Vec<Statement>,
}

impl<'b, 'a> Frame<'b, 'a> {
    pub fn lambda(&mut self, node: &syntax::Expression) -> (Expression, Type) {
        let label = self.engine.fresh("fn");
        let sym = self.engine.symbol(&self.stem, &label);
        let empty = BTreeMap::new();
        let spec = Spec {
            expr: node,
            subst: &self.subst,
            constants: &empty,
            exported: false,
        };
        let routine = self.engine.callable((&self.stem, sym.clone()), &spec);
        let ins = routine.inputs.iter().map(|(_, t)| t.clone()).collect();
        let out = match routine.asynchronous {
            true => Type::Fiber(Box::new(routine.output.clone())),
            false => routine.output.clone(),
        };
        self.engine.routines.push(routine);
        let fkind = Type::Function {
            inputs: ins,
            output: Box::new(out),
        };
        (Expression::Symbol(sym, fkind.clone()), fkind)
    }

    pub fn question(&mut self, inner: &syntax::Expression) -> (Expression, Type) {
        let (sub, skind) = self.expr(inner, None);
        let Type::Choice(cname) = &skind else {
            return (sub, skind);
        };
        let catalog = &self.engine.typing.catalog;
        let Some(tagged) = catalog.choices.get(cname).cloned() else {
            return (sub, skind);
        };
        let ok = tagged.arms.first().cloned();
        let err = tagged.arms.get(1).cloned();
        let vslot = ok.as_ref().and_then(|a| a.payloads.first()).cloned();
        let vtype = vslot.as_ref().map_or(Type::Void, |f| f.kind.clone());
        let eslot = err.as_ref().and_then(|a| a.payloads.first()).cloned();
        let etype = eslot.as_ref().map_or(Type::Void, |f| f.kind.clone());
        let vname = self.engine.fresh("ok");
        let ename = self.engine.fresh("err");
        let outer = match &self.expected {
            Type::Choice(oc) => oc.clone(),
            _ => cname.clone(),
        };
        let otag = self
            .engine
            .typing
            .catalog
            .choices
            .get(&outer)
            .and_then(|t| t.arms.get(1))
            .map_or(1, |a| a.tag);
        let mut unwind = self.unwind(0);
        unwind.push(Statement::Result(Expression::Variant {
            choice: outer.clone(),
            arm: err
                .as_ref()
                .map_or_else(|| String::from("err"), |a| a.name.clone()),
            tag: otag,
            payloads: eslot
                .as_ref()
                .map(|_| vec![Expression::Local(ename.clone(), etype)])
                .unwrap_or_default(),
            kind: Type::Choice(outer),
        }));
        let cases = vec![
            Case {
                tags: vec![ok.as_ref().map_or(0, |a| a.tag)],
                bindings: vslot.into_iter().map(|f| (vname.clone(), f)).collect(),
                body: Expression::Local(vname, vtype.clone()),
            },
            Case {
                tags: vec![err.as_ref().map_or(1, |a| a.tag)],
                bindings: eslot.into_iter().map(|f| (ename.clone(), f)).collect(),
                body: Expression::Block {
                    body: unwind,
                    tail: None,
                    kind: vtype.clone(),
                },
            },
        ];
        (
            Expression::Switch {
                subject: Box::new(sub),
                cases,
                kind: vtype.clone(),
            },
            vtype,
        )
    }

    pub fn mapped(
        &mut self,
        base: &syntax::Expression,
        args: &[syntax::Expression],
    ) -> Option<(Expression, Type)> {
        let [syntax::Expression::Function { inputs, body, .. }] = args else {
            return None;
        };
        let param = inputs
            .first()
            .map_or_else(|| String::from("item"), |p| p.name.clone());
        let (raw, bkind) = self.expr(base, None);
        let (src, peeled) = load(raw, bkind);
        let input = match &peeled {
            Type::Slice(e) => Type::Pointer(e.clone()),
            Type::List(e) => *e.clone(),
            _ => Type::Unsigned(8),
        };
        let holder = self.engine.fresh("src");
        let count = self.engine.fresh("len");
        let buf = self.engine.fresh("buf");
        let idx = self.engine.fresh("idx");
        self.push();
        self.declare(&param, input.clone());
        let (mapped, output) = match body {
            syntax::Body::Block(b) => self.scoped(b, None),
            syntax::Body::Switch(d) => self.switch(d, None),
            syntax::Body::Bucket(p) => self.bucket(p, None),
            syntax::Body::Loop(l) => self.repetition(l, None),
        };
        let mut drops = Vec::new();
        self.pop(&mut drops);
        let list = Type::List(Box::new(output.clone()));
        let steps = maploop(Mapping {
            param,
            holder: holder.clone(),
            count: count.clone(),
            buf: buf.clone(),
            idx: idx.clone(),
            peeled: peeled.clone(),
            input,
            output: output.clone(),
            mapped,
            drops,
        });
        let stmts = vec![
            Statement::Bind {
                name: holder.clone(),
                mutable: false,
                kind: peeled.clone(),
                value: src,
            },
            Statement::Bind {
                name: count.clone(),
                mutable: false,
                kind: Type::Unsigned(8),
                value: Expression::Length(Box::new(Expression::Local(holder, peeled))),
            },
            Statement::Bind {
                name: buf.clone(),
                mutable: true,
                kind: list.clone(),
                value: Expression::Buffer {
                    count: Box::new(Expression::Local(count, Type::Unsigned(8))),
                    element: output,
                    kind: list.clone(),
                },
            },
            Statement::Bind {
                name: idx,
                mutable: true,
                kind: Type::Unsigned(8),
                value: Expression::Int(0, Type::Unsigned(8)),
            },
            Statement::Evaluate(Expression::Loop {
                body: steps,
                kind: Type::Void,
            }),
        ];
        Some((
            Expression::Block {
                body: stmts,
                tail: Some(Box::new(Expression::Local(buf, list.clone()))),
                kind: list.clone(),
            },
            list,
        ))
    }
}

fn maploop(mut plan: Mapping) -> Vec<Statement> {
    let list = Type::List(Box::new(plan.output.clone()));
    let elem = match &plan.peeled {
        Type::Slice(e) | Type::List(e) => *e.clone(),
        _ => Type::Unsigned(8),
    };
    let indexed = Expression::Index {
        base: Box::new(Expression::Local(plan.holder, plan.peeled.clone())),
        index: Box::new(Expression::Local(plan.idx.clone(), Type::Unsigned(8))),
        kind: elem,
    };
    let bound = match matches!(&plan.peeled, Type::Slice(_)) {
        true => Expression::Borrow {
            mutable: false,
            target: Box::new(indexed),
            kind: plan.input.clone(),
        },
        false => indexed,
    };
    let mut steps = vec![
        Statement::Evaluate(Expression::Branch {
            condition: Box::new(Expression::Binary {
                operator: String::from(">="),
                left: Box::new(Expression::Local(plan.idx.clone(), Type::Unsigned(8))),
                right: Box::new(Expression::Local(plan.count, Type::Unsigned(8))),
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
            name: plan.param,
            mutable: false,
            kind: plan.input,
            value: bound,
        },
        Statement::Store {
            target: Expression::Index {
                base: Box::new(Expression::Local(plan.buf, list)),
                index: Box::new(Expression::Local(plan.idx.clone(), Type::Unsigned(8))),
                kind: plan.output,
            },
            value: plan.mapped,
        },
    ];
    steps.append(&mut plan.drops);
    steps.push(Statement::Assign {
        name: plan.idx.clone(),
        value: Expression::Binary {
            operator: String::from("+"),
            left: Box::new(Expression::Local(plan.idx, Type::Unsigned(8))),
            right: Box::new(Expression::Int(1, Type::Unsigned(8))),
            kind: Type::Unsigned(8),
        },
    });
    steps
}

pub fn qualified(
    base: &syntax::Expression,
    stem: &str,
    imports: &BTreeMap<(String, String), (String, bool)>,
) -> Option<(String, String)> {
    match base {
        syntax::Expression::Word { text, .. }
            if text.chars().next().is_some_and(|c| c.is_ascii_uppercase()) =>
        {
            Some((stem.to_owned(), text.clone()))
        }
        syntax::Expression::Member {
            base: prefix,
            field,
            ..
        } if field.chars().next().is_some_and(|c| c.is_ascii_uppercase()) => {
            let syntax::Expression::Word { text, .. } = prefix.as_ref() else {
                return None;
            };
            let target = imports
                .get(&(stem.to_owned(), text.clone()))
                .map_or_else(|| text.clone(), |(l, _)| l.clone());
            Some((target, field.clone()))
        }
        _ => None,
    }
}
