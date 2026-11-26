use crate::front::numeric::Value;
use crate::front::types::Map;
use crate::layout::Type;
use crate::lower::engine::Engine;
use crate::tree::{Expression, Statement};
use simplex::tree as syntax;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Local {
    pub name: String,
    pub kind: Type,
    pub dropper: Option<(String, bool)>,
    pub moved: bool,
}

#[derive(Clone)]
pub struct Unrolled {
    pub name: String,
    pub tags: Vec<String>,
}

pub struct Frame<'b, 'a> {
    pub engine: &'b mut Engine<'a>,
    pub stem: String,
    pub expected: Type,
    pub subst: Map,
    pub constants: BTreeMap<String, Value>,
    pub scopes: Vec<Vec<Local>>,
    pub loops: Vec<usize>,
    pub breaks: Vec<Option<Type>>,
    pub targets: Vec<Option<Type>>,
    pub fields: BTreeMap<String, Unrolled>,
}

impl<'b, 'a> Frame<'b, 'a> {
    pub fn new(
        engine: &'b mut Engine<'a>,
        stem: &str,
        expected: Type,
        constants: BTreeMap<String, Value>,
    ) -> Self {
        Self::with(engine, stem, (expected, Map::new()), constants)
    }

    pub fn with(
        engine: &'b mut Engine<'a>,
        stem: &str,
        ctx: (Type, Map),
        constants: BTreeMap<String, Value>,
    ) -> Self {
        Self {
            engine,
            stem: stem.to_owned(),
            expected: ctx.0,
            subst: ctx.1,
            constants,
            scopes: vec![Vec::new()],
            loops: Vec::new(),
            breaks: Vec::new(),
            targets: Vec::new(),
            fields: BTreeMap::new(),
        }
    }

    pub fn lookup(&self, name: &str) -> Option<Type> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|slot| slot.name == name)
            .map(|slot| slot.kind.clone())
    }

    pub fn consume(&mut self, name: &str) {
        let Some(kind) = self.lookup(name) else {
            return;
        };
        if self.engine.copyable(&self.stem, &kind) {
            return;
        }
        if let Some(slot) = self
            .scopes
            .iter_mut()
            .rev()
            .flat_map(|s| s.iter_mut().rev())
            .find(|slot| slot.name == name)
        {
            slot.moved = true;
        }
    }

    pub fn revive(&mut self, name: &str) {
        if let Some(slot) = self
            .scopes
            .iter_mut()
            .rev()
            .flat_map(|s| s.iter_mut().rev())
            .find(|slot| slot.name == name)
        {
            slot.moved = false;
        }
    }

    pub fn function(
        &mut self,
        params: &[(String, Type)],
        body: &syntax::Body,
    ) -> (Vec<Statement>, Type) {
        for (name, kind) in params {
            self.declare(name, kind.clone());
        }
        let target = self.expected.clone();
        let hint = (!matches!(target, Type::Void)).then_some(&target);
        match body {
            syntax::Body::Block(block) => self.routine(&block.items),
            syntax::Body::Switch(d) => {
                let (expr, kind) = self.switch(d, hint);
                (self.finish(expr, &kind), kind)
            }
            syntax::Body::Bucket(p) => {
                let (expr, kind) = self.bucket(p, hint);
                (self.finish(expr, &kind), kind)
            }
            syntax::Body::Loop(l) => {
                let (expr, kind) = self.repetition(l, hint);
                (self.finish(expr, &kind), kind)
            }
        }
    }

    fn finish(&mut self, expr: Expression, kind: &Type) -> Vec<Statement> {
        let mut out = Vec::new();
        if matches!(kind, Type::Void) {
            if !matches!(expr, Expression::Void) {
                out.push(Statement::Evaluate(expr));
            }
            self.pop(&mut out);
            return out;
        }
        let temp = self.engine.fresh("ret");
        out.push(Statement::Bind {
            name: temp.clone(),
            mutable: false,
            kind: kind.clone(),
            value: expr,
        });
        self.pop(&mut out);
        out.push(Statement::Result(Expression::Local(temp, kind.clone())));
        out
    }

    fn routine(&mut self, items: &[syntax::Item]) -> (Vec<Statement>, Type) {
        let mut out = Vec::new();
        let Some((last, head)) = items.split_last() else {
            self.pop(&mut out);
            return (out, Type::Void);
        };
        for item in head {
            self.item(item, &mut out);
        }
        let expected = self.expected.clone();
        let hint = (!matches!(expected, Type::Void)).then_some(&expected);
        if let Some((val, kind)) = self.tail(last, hint) {
            out.append(&mut self.finish(val, &kind));
            return (out, kind);
        }
        self.item(last, &mut out);
        self.pop(&mut out);
        (out, Type::Void)
    }

    pub fn script(&mut self, items: &[syntax::Item]) -> Vec<Statement> {
        let mut out = Vec::new();
        for item in items {
            self.item(item, &mut out);
        }
        self.pop(&mut out);
        out
    }

    pub fn scoped(&mut self, block: &syntax::Block, expected: Option<&Type>) -> (Expression, Type) {
        self.push();
        let mut body = Vec::new();
        let Some((last, head)) = block.items.split_last() else {
            self.pop(&mut body);
            return empty(body);
        };
        for item in head {
            self.item(item, &mut body);
        }
        if let syntax::Statement::Evaluate(syntax::Expression::Word { text, .. }) = &last.statement
            && self.jump(text, &mut body)
        {
            self.scopes.pop();
            return empty(body);
        }
        if let Some((val, kind)) = self.tail(last, expected) {
            if matches!(kind, Type::Void) {
                if !matches!(val, Expression::Void) {
                    body.push(Statement::Evaluate(val));
                }
                self.pop(&mut body);
                return empty(body);
            }
            let temp = self.engine.fresh("blk");
            body.push(Statement::Bind {
                name: temp.clone(),
                mutable: false,
                kind: kind.clone(),
                value: val,
            });
            self.pop(&mut body);
            return (
                Expression::Block {
                    body,
                    tail: Some(Box::new(Expression::Local(temp, kind.clone()))),
                    kind: kind.clone(),
                },
                kind,
            );
        }
        self.item(last, &mut body);
        self.pop(&mut body);
        empty(body)
    }

    fn tail(&mut self, last: &syntax::Item, expected: Option<&Type>) -> Option<(Expression, Type)> {
        match &last.statement {
            syntax::Statement::Loop(kind) => Some(self.repetition(kind, expected)),
            syntax::Statement::Evaluate(syntax::Expression::Meta(_)) => None,
            syntax::Statement::Evaluate(syntax::Expression::Word { text, .. })
                if matches!(text.as_str(), "continue" | "break") =>
            {
                None
            }
            syntax::Statement::Evaluate(expr) => Some(self.expr(expr, expected)),
            _ => None,
        }
    }

    pub fn item(&mut self, item: &syntax::Item, out: &mut Vec<Statement>) {
        match &item.statement {
            syntax::Statement::Bind(b) => self.bind(b, out),
            syntax::Statement::Assign {
                target,
                operator,
                value,
            } => self.assign((target, operator, value), out),
            syntax::Statement::For {
                constant,
                item: name,
                source,
                body,
            } => match constant {
                true => self.unroll((name, source, &body.items), out),
                false => self.iterate((name, source, body), out),
            },
            syntax::Statement::Loop(kind) => {
                let (expr, _) = self.repetition(kind, Some(&Type::Void));
                out.push(Statement::Evaluate(expr));
            }
            syntax::Statement::Evaluate(syntax::Expression::Meta(block)) => {
                let mut eval = self.engine.evaluator(&self.stem, self.subst.clone());
                eval.vars.extend(self.constants.clone());
                eval.block(block);
            }
            syntax::Statement::Evaluate(syntax::Expression::Word { text, .. })
                if self.jump(text, out) => {}
            syntax::Statement::Evaluate(expr) => {
                let (val, _) = self.expr(expr, None);
                out.push(Statement::Evaluate(val));
            }
            _ => {}
        }
    }
}

fn empty(body: Vec<Statement>) -> (Expression, Type) {
    (
        Expression::Block {
            body,
            tail: None,
            kind: Type::Void,
        },
        Type::Void,
    )
}
