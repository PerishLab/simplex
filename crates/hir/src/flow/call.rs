use crate::flow::fallible::qualified;
use crate::front::numeric::Value;
use crate::layout::{Mangle, Type};
use crate::lower::coerce::load;
use crate::lower::frame::Frame;
use crate::tree::{Expression, Routine, Statement};
use simplex::tree as syntax;

impl<'b, 'a> Frame<'b, 'a> {
    pub fn invoke(
        &mut self,
        node: &syntax::Expression,
        expected: Option<&Type>,
    ) -> (Expression, Type) {
        let syntax::Expression::Call {
            callee,
            generics,
            arguments,
            line,
            column,
            ..
        } = node
        else {
            return (Expression::Void, Type::Void);
        };
        if let syntax::Expression::Word { text, .. } = callee.as_ref() {
            if let Some(hit) = self.builtin(text, generics, arguments) {
                return hit;
            }
            let stem = self.stem.clone();
            return self.direct(&stem, text, (generics, arguments), (*line, *column));
        }
        if let syntax::Expression::Member {
            base,
            field,
            generics: mgen,
            ..
        } = callee.as_ref()
        {
            let chosen = match mgen.is_empty() {
                true => generics.as_slice(),
                false => mgen.as_slice(),
            };
            if let Some(hit) = self.postfix(base, field, (chosen, arguments), expected) {
                return hit;
            }
            return self.route(base, field, (chosen, arguments), (*line, *column));
        }
        let (pointer, fkind) = self.expr(callee, None);
        let (ins, out) = match fkind {
            Type::Function { inputs, output } => (inputs, *output),
            _ => (Vec::new(), Type::Unsigned(8)),
        };
        let args = self.arguments(arguments, &ins);
        (
            Expression::Indirect {
                callee: Box::new(pointer),
                arguments: args,
                kind: out.clone(),
            },
            out,
        )
    }

    fn builtin(
        &mut self,
        name: &str,
        generics: &[syntax::Spec],
        args: &[syntax::Expression],
    ) -> Option<(Expression, Type)> {
        match name {
            "yield" => Some((Expression::Yield, Type::Void)),
            "assert" => Some((Expression::Void, Type::Void)),
            "caller" => {
                let kind = self.engine.typing.origin();
                Some((
                    Expression::Local(String::from("$caller"), kind.clone()),
                    kind,
                ))
            }
            "embed" => Some(self.embedded(args)),
            "size" | "fields" | "variants" => Some(self.reflected(name, generics)),
            "has" => Some(self.marked(args)),
            "get" => Some(self.projected(args)),
            _ => None,
        }
    }

    fn embedded(&mut self, args: &[syntax::Expression]) -> (Expression, Type) {
        let Some(syntax::Expression::Text(raw)) = args.first() else {
            return (Expression::Void, Type::Void);
        };
        let ns = self.engine.ns.clone();
        let root = self.engine.root.clone();
        let sym = self
            .engine
            .pool
            .embed((&ns, &self.stem), &root, raw)
            .unwrap_or_default();
        (
            Expression::Bytes(sym),
            Type::Slice(Box::new(Type::Unsigned(1))),
        )
    }

    fn reflected(&mut self, name: &str, generics: &[syntax::Spec]) -> (Expression, Type) {
        let call = syntax::Expression::Call {
            callee: Box::new(syntax::Expression::Word {
                text: name.to_owned(),
                line: 1,
                column: 1,
            }),
            generics: generics.to_vec(),
            arguments: Vec::new(),
            multiline: false,
            line: 1,
            column: 1,
        };
        let mut eval = self.engine.evaluator(&self.stem, self.subst.clone());
        eval.eval(&call).into()
    }

    fn marked(&self, args: &[syntax::Expression]) -> (Expression, Type) {
        let [
            syntax::Expression::Word { text: fvar, .. },
            syntax::Expression::Word { text: tag, .. },
        ] = args
        else {
            return (Expression::Bool(false), Type::Bool);
        };
        let hit = self.fields.get(fvar).is_some_and(|u| u.tags.contains(tag));
        (Expression::Bool(hit), Type::Bool)
    }

    fn projected(&mut self, args: &[syntax::Expression]) -> (Expression, Type) {
        let [target, syntax::Expression::Word { text: fvar, .. }] = args else {
            return (Expression::Void, Type::Void);
        };
        let fname = self
            .fields
            .get(fvar)
            .map(|u| u.name.clone())
            .unwrap_or_default();
        self.projection(target, &fname, &[])
    }

    fn postfix(
        &mut self,
        base: &syntax::Expression,
        field: &str,
        call: (&[syntax::Spec], &[syntax::Expression]),
        expected: Option<&Type>,
    ) -> Option<(Expression, Type)> {
        let (generics, args) = call;
        match field {
            "into" | "as" => {
                let (val, _) = self.expr(base, None);
                let out = generics
                    .first()
                    .map(|s| self.engine.typing.resolve(&self.stem, s, &self.subst))
                    .unwrap_or(Type::Unsigned(8));
                return Some((
                    Expression::Cast {
                        value: Box::new(val),
                        kind: out.clone(),
                    },
                    out,
                ));
            }
            "len" => return Some(self.length(base)),
            "map" => return self.mapped(base, args),
            "all" | "race" if args.is_empty() => return Some(self.batch(base, field)),
            _ => {}
        }
        let (owner, cname) = qualified(base, &self.stem, &self.engine.world.imports)?;
        Some(self.construct((&owner, &cname), field, args, expected))
    }

    fn length(&mut self, base: &syntax::Expression) -> (Expression, Type) {
        if let syntax::Expression::Call { callee, .. } = base
            && let syntax::Expression::Word { text, .. } = callee.as_ref()
            && matches!(text.as_str(), "fields" | "variants")
        {
            let mut eval = self.engine.evaluator(&self.stem, self.subst.clone());
            if let Value::Count(n) = eval.eval(base) {
                return (
                    Expression::Int(n as i128, Type::Unsigned(8)),
                    Type::Unsigned(8),
                );
            }
        }
        let (raw, bkind) = self.place(base);
        let (src, _) = load(raw, bkind);
        (Expression::Length(Box::new(src)), Type::Unsigned(8))
    }

    fn batch(&mut self, base: &syntax::Expression, op: &str) -> (Expression, Type) {
        let (fibers, lkind) = self.expr(base, None);
        let elem = match &lkind {
            Type::List(inner) => match inner.as_ref() {
                Type::Fiber(t) => *t.clone(),
                other => other.clone(),
            },
            _ => Type::Unsigned(8),
        };
        let out = match op {
            "all" => Type::List(Box::new(elem.clone())),
            _ => elem.clone(),
        };
        let label = self.engine.fresh(op);
        let sym = Mangle::symbol(&self.engine.ns, &self.stem, &label);
        let arg = Expression::Local(String::from("fibers"), lkind.clone());
        let inner = match op {
            "all" => Expression::All {
                fibers: Box::new(arg),
                element: elem,
                kind: out.clone(),
            },
            _ => Expression::Race {
                fibers: Box::new(arg),
                kind: out.clone(),
            },
        };
        self.engine.routines.push(Routine {
            symbol: sym.clone(),
            exported: false,
            asynchronous: true,
            inputs: vec![(String::from("fibers"), lkind)],
            output: out.clone(),
            body: vec![Statement::Result(inner)],
        });
        let fkind = Type::Fiber(Box::new(out));
        (
            Expression::Call {
                symbol: sym,
                arguments: vec![fibers],
                kind: fkind.clone(),
            },
            fkind,
        )
    }

    pub fn arguments(&mut self, args: &[syntax::Expression], expected: &[Type]) -> Vec<Expression> {
        let mut out = Vec::new();
        for (idx, arg) in args.iter().enumerate() {
            let hint = expected.get(idx);
            let (raw, ak) = self.expr(arg, hint);
            let coerced = match hint {
                Some(want) => self.coerce(raw, &ak, want),
                None => raw,
            };
            out.push(coerced);
        }
        out
    }
}
