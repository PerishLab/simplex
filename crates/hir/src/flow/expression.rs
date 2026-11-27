use crate::eval::asset::{byte, unescape};
use crate::front::numeric::{digits, float, narrow};
use crate::layout::Type;
use crate::lower::frame::Frame;
use crate::tree::Expression;
use simplex::tree as syntax;

impl<'b, 'a> Frame<'b, 'a> {
    pub fn expr(
        &mut self,
        node: &syntax::Expression,
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        match node {
            syntax::Expression::Number(text) => self.number(text, expected),
            syntax::Expression::Text(raw) => {
                let (ns, module) = self.engine.world.coords(&self.engine.ns, &self.stem);
                let (ns, module) = (ns.to_owned(), module.to_owned());
                let sym = self.engine.pool.intern((&ns, &module), unescape(raw));
                (
                    Expression::Bytes(sym),
                    Type::Slice(Box::new(Type::Unsigned(1))),
                )
            }
            syntax::Expression::Byte(raw) => (
                Expression::Int(byte(raw) as i128, Type::Unsigned(1)),
                Type::Unsigned(1),
            ),
            syntax::Expression::Word { text, .. } => self.word(text, true),
            syntax::Expression::Group(inner) => self.expr(inner, expected),
            syntax::Expression::Try(inner) => self.question(inner),
            syntax::Expression::Unary { operator, operand } => {
                let (val, kind) = self.expr(operand, expected);
                (
                    Expression::Unary {
                        operator: operator.clone(),
                        operand: Box::new(val),
                        kind: kind.clone(),
                    },
                    kind,
                )
            }
            syntax::Expression::Binary {
                left,
                operator,
                right,
                ..
            } => self.binary(operator, (left, right), expected),
            syntax::Expression::Borrow {
                mutable, target, ..
            } => self.borrowed(*mutable, target, expected),
            syntax::Expression::Index {
                base, start, limit, ..
            } => self.indexed(base, start, limit.as_deref()),
            syntax::Expression::Member {
                base,
                field,
                generics,
                ..
            } => self.member((base, field), generics, expected),
            syntax::Expression::Call { .. } => self.invoke(node, expected),
            syntax::Expression::Record {
                module,
                name,
                fields,
                ..
            } => self.record((module.as_deref(), name), fields, expected),
            syntax::Expression::List { items, .. } => self.listed(items, expected),
            syntax::Expression::Meta(block) => {
                let mut eval = self.engine.evaluator(&self.stem, self.subst.clone());
                eval.vars.extend(self.constants.clone());
                eval.block(block).into()
            }
            syntax::Expression::Switch(d) => self.switch(d, expected),
            syntax::Expression::Bucket(p) => self.bucket(p, expected),
            syntax::Expression::Loop(k) => self.repetition(k, expected),
            syntax::Expression::Function { .. } => self.lambda(node),
        }
    }

    pub fn place(&mut self, node: &syntax::Expression) -> (Expression, Type) {
        match node {
            syntax::Expression::Word { text, .. } => self.word(text, false),
            syntax::Expression::Group(inner) => self.place(inner),
            syntax::Expression::Member {
                base,
                field,
                generics,
                ..
            } => self.projection(base, field, generics),
            syntax::Expression::Index {
                base, start, limit, ..
            } => self.indexed(base, start, limit.as_deref()),
            other => self.expr(other, None),
        }
    }

    fn number(&self, text: &str, expected: Option<&Type>) -> (Expression, Type) {
        if text.contains('.') {
            let kind = match expected {
                Some(Type::Float(n)) => Type::Float(*n),
                _ => Type::Float(8),
            };
            return (Expression::Float(float(text, &kind), kind.clone()), kind);
        }
        let kind = match expected {
            Some(Type::Signed(n)) => Type::Signed(*n),
            Some(Type::Unsigned(n)) => Type::Unsigned(*n),
            _ => Type::Signed(8),
        };
        (
            Expression::Int(narrow(digits(text), &kind), kind.clone()),
            kind,
        )
    }

    fn word(&mut self, text: &str, consume: bool) -> (Expression, Type) {
        match text {
            "true" => return (Expression::Bool(true), Type::Bool),
            "false" => return (Expression::Bool(false), Type::Bool),
            "continue" | "break" => {
                let mut stmts = Vec::new();
                self.jump(text, &mut stmts);
                return (
                    Expression::Block {
                        body: stmts,
                        tail: None,
                        kind: Type::Void,
                    },
                    Type::Void,
                );
            }
            _ => {}
        }
        let globals = &self.engine.globals;
        if let Some(val) = self
            .constants
            .get(text)
            .or_else(|| globals.get(&(self.stem.clone(), text.to_owned())))
            .cloned()
        {
            return val.into();
        }
        let embedded = &self.engine.embedded;
        if let Some(sym) = embedded.get(&(self.stem.clone(), text.to_owned())).cloned() {
            return (
                Expression::Bytes(sym),
                Type::Slice(Box::new(Type::Unsigned(1))),
            );
        }
        if let Some(kind) = self.lookup(text) {
            if consume {
                self.consume(text);
            }
            return (Expression::Local(text.to_owned(), kind.clone()), kind);
        }
        let stem = self.stem.clone();
        self.functionref(&stem, text)
    }

    pub fn functionref(&mut self, owner: &str, text: &str) -> (Expression, Type) {
        let bindings = &self.engine.world.bindings;
        if let Some(b) = bindings.get(&(owner.to_owned(), text.to_owned())).cloned()
            && let syntax::Expression::Function {
                asynchronous,
                inputs,
                output,
                ..
            } = &b.value
        {
            let ins = inputs
                .iter()
                .map(|p| {
                    self.engine
                        .typing
                        .optional(owner, p.annotation.as_ref(), &self.subst)
                })
                .collect();
            let raw = match output {
                Some(spec) => self.engine.typing.resolve(owner, spec, &self.subst),
                None => self.engine.ensure(owner, text).unwrap_or(Type::Void),
            };
            let out = match asynchronous {
                true => Type::Fiber(Box::new(raw)),
                false => raw,
            };
            let fkind = Type::Function {
                inputs: ins,
                output: Box::new(out),
            };
            let sym = self.engine.symbol(owner, text);
            return (Expression::Symbol(sym, fkind.clone()), fkind);
        }
        (
            Expression::Local(text.to_owned(), Type::Signed(8)),
            Type::Signed(8),
        )
    }

    fn binary(
        &mut self,
        op: &str,
        pair: (&syntax::Expression, &syntax::Expression),
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let hint = match matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||") {
            true => None,
            false => expected,
        };
        let (mut lhs, mut lk) = self.expr(pair.0, hint);
        let (mut rhs, rk) = self.expr(pair.1, Some(&lk));
        if matches!(pair.0, syntax::Expression::Number(_))
            && !matches!(pair.1, syntax::Expression::Number(_))
        {
            lhs = self.coerce(lhs, &lk, &rk);
            lk = rk;
        } else {
            rhs = self.coerce(rhs, &rk, &lk);
        }
        let out = match op {
            "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => Type::Bool,
            _ => lk,
        };
        (
            Expression::Binary {
                operator: op.to_owned(),
                left: Box::new(lhs),
                right: Box::new(rhs),
                kind: out.clone(),
            },
            out,
        )
    }

    fn borrowed(
        &mut self,
        mutable: bool,
        target: &syntax::Expression,
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        if let syntax::Expression::Index {
            base,
            start,
            limit: Some(end),
            ..
        } = target
        {
            return self.window(base, start, end);
        }
        let (inner, tkind) = self.place(target);
        let pkind = Type::Pointer(Box::new(tkind));
        let ptr = Expression::Borrow {
            mutable,
            target: Box::new(inner),
            kind: pkind.clone(),
        };
        match expected {
            Some(want) if matches!(want, Type::Trait(_) | Type::Slice(_)) => {
                (self.coerce(ptr, &pkind, want), want.clone())
            }
            _ => (ptr, pkind),
        }
    }
}
