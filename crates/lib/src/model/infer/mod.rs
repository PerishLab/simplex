mod call;

use super::check::{Scope, Type, compatible, unify};
use crate::code::{Code, Finding};
use crate::tree::{Expression, Spec};

impl<'a> Scope<'a> {
    pub(crate) fn infer(&mut self, expr: &Expression) -> Result<Type, Finding> {
        match expr {
            Expression::Number(text) => Ok(if text.contains('.') {
                Type::Float
            } else {
                Type::Integer
            }),
            Expression::Text(_) => Ok(Type::Scalar("Text".into())),
            Expression::Byte(_) => Ok(Type::Scalar("u8".into())),
            Expression::Word { text, .. } => Ok(self.word(text)),
            Expression::Group(inner) => self.infer(inner),
            Expression::Try(inner) => {
                let kind = self.infer(inner)?;
                Ok(match kind {
                    Type::Nominal(_) => Type::Unknown,
                    other => other,
                })
            }
            Expression::List {
                items,
                line,
                column,
                ..
            } => self.listed(items, (*line, *column)),
            Expression::Unary { operand, .. } => self.infer(operand),
            Expression::Borrow {
                mutable, target, ..
            } => Ok(Type::Borrow {
                mutable: *mutable,
                inner: Box::new(self.infer(target)?),
            }),
            Expression::Index {
                base, start, limit, ..
            } => self.indexed(base, start, limit.as_deref()),
            Expression::Binary { .. } => self.binary(expr),
            Expression::Member {
                base,
                field,
                generics,
                ..
            } => self.member(base, field, generics),
            Expression::Call { .. } => self.call(expr),
            Expression::Meta(block) => self.block(block),
            Expression::Record {
                module,
                name,
                fields,
                ..
            } => {
                for field in fields {
                    self.infer(&field.value)?;
                }
                Ok(Type::Nominal(module.as_ref().map_or_else(
                    || self.table.resolve(name),
                    |p| format!("{p}.{name}"),
                )))
            }
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            Expression::Function { .. } => self.callable(expr),
        }
    }

    fn listed(&mut self, items: &[Expression], pos: (usize, usize)) -> Result<Type, Finding> {
        let mut element = Type::Unknown;
        for item in items {
            let current = self.infer(item)?;
            if !compatible(&element, &current) {
                return Err(Finding {
                    code: Code::Coercion,
                    line: pos.0,
                    column: pos.1,
                });
            }
            element = unify(element, current);
        }
        Ok(Type::List(Box::new(element)))
    }

    fn indexed(
        &mut self,
        base: &Expression,
        start: &Expression,
        limit: Option<&Expression>,
    ) -> Result<Type, Finding> {
        let container = self.infer(base)?;
        self.infer(start)?;
        if let Some(end) = limit {
            self.infer(end)?;
        }
        let peeled = match container {
            Type::Borrow { inner, .. } => *inner,
            other => other,
        };
        match peeled {
            Type::Slice(elem) => Ok(if limit.is_some() {
                Type::Slice(elem)
            } else {
                *elem
            }),
            Type::List(elem) => Ok(if limit.is_some() {
                Type::List(elem)
            } else {
                *elem
            }),
            _ => Ok(Type::Unknown),
        }
    }

    fn word(&self, text: &str) -> Type {
        if matches!(text, "true" | "false") {
            return Type::Scalar("bool".into());
        }
        self.locals.get(text).cloned().unwrap_or(Type::Unknown)
    }

    fn binary(&mut self, expr: &Expression) -> Result<Type, Finding> {
        let Expression::Binary {
            left,
            operator,
            right,
            line,
            column,
            ..
        } = expr
        else {
            return Ok(Type::Unknown);
        };
        let lhs = self.infer(left)?;
        let rhs = self.infer(right)?;
        if !compatible(&lhs, &rhs) {
            return Err(Finding {
                code: Code::Coercion,
                line: *line,
                column: *column,
            });
        }
        if matches!(
            operator.as_str(),
            "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
        ) {
            return Ok(Type::Scalar("bool".into()));
        }
        Ok(unify(lhs, rhs))
    }

    fn member(
        &mut self,
        base: &Expression,
        field: &str,
        generics: &[Spec],
    ) -> Result<Type, Finding> {
        if let Expression::Word { text, .. } = base
            && text
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            return Ok(Type::Nominal(self.table.resolve(text)));
        }
        let owner = self.infer(base)?;
        if field == "await" {
            return Ok(match owner {
                Type::Fiber(inner) => *inner,
                _ => Type::Unknown,
            });
        }
        if matches!(field, "all" | "race")
            && let Type::List(elem) = &owner
            && let Type::Fiber(inner) = elem.as_ref()
        {
            return Ok(if field == "all" {
                Type::Fiber(Box::new(Type::List(inner.clone())))
            } else {
                Type::Fiber(inner.clone())
            });
        }
        if matches!(field, "into" | "as")
            && let [target] = generics
        {
            return Ok(self.from(target));
        }
        self.projected(owner, field)
    }
}
