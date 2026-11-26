use super::Signature;
use super::check::{Scope, Type, compatible, unify};
use crate::code::{Code, Finding};
use crate::tree::{Expression, Generic, Spec};

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
            Expression::Group(inner) | Expression::Try(inner) => self.infer(inner),
            Expression::Unary { operand, .. } => self.infer(operand),
            Expression::Binary { .. } => self.binary(expr),
            Expression::Member {
                base,
                field,
                generics,
                ..
            } => self.member(base, field, generics),
            Expression::Call { .. } => self.call(expr),
            Expression::Record {
                module,
                name,
                fields,
                ..
            } => {
                for field in fields {
                    self.infer(&field.value)?;
                }
                let label = match module {
                    Some(prefix) => format!("{prefix}.{name}"),
                    None => self.table.resolve(name),
                };
                Ok(Type::Nominal(label))
            }
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            Expression::Function { .. } => self.callable(expr),
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
        if matches!(field, "into" | "as")
            && let [target] = generics
        {
            return Ok(self.from(target));
        }
        if let Type::Nominal(name) = owner {
            if let Some(spec) = self.table.field(&name, field) {
                return Ok(self.from(&spec));
            }
            if let Some(sig) = self.table.method(&name, field)
                && let Some(out) = &sig.output
            {
                return Ok(self.from(out));
            }
        }
        Ok(Type::Unknown)
    }

    fn call(&mut self, expr: &Expression) -> Result<Type, Finding> {
        let Expression::Call {
            callee,
            arguments,
            line,
            column,
            ..
        } = expr
        else {
            return Ok(Type::Unknown);
        };
        let mut actuals = Vec::new();
        for arg in arguments {
            actuals.push(self.infer(arg)?);
        }
        if let Expression::Word { text, .. } = callee.as_ref()
            && let Some(sig) = self.table.functions.get(text)
        {
            self.bounds(sig, &actuals, (*line, *column))?;
            if let Some(out) = &sig.output {
                return Ok(self.from(out));
            }
        }
        self.infer(callee)
    }

    fn bounds(
        &self,
        sig: &Signature,
        actuals: &[Type],
        pos: (usize, usize),
    ) -> Result<(), Finding> {
        for (spec, actual) in sig.inputs.iter().zip(actuals) {
            let Some(Spec::Named { name: param, .. }) = spec else {
                continue;
            };
            if let Some(clause) = sig.generics.iter().find(|item| item.name == *param) {
                self.satisfies(clause, actual, pos)?;
            }
        }
        Ok(())
    }

    fn satisfies(
        &self,
        clause: &Generic,
        actual: &Type,
        pos: (usize, usize),
    ) -> Result<(), Finding> {
        let Type::Nominal(concrete) = actual else {
            return Ok(());
        };
        if !self.table.structs.contains_key(concrete) && !self.table.enums.contains_key(concrete) {
            return Ok(());
        }
        for bound in &clause.bounds {
            let target = self.table.resolve(bound);
            if !self.table.satisfied.contains(&(concrete.clone(), target)) {
                return Err(Finding {
                    code: Code::Conformance,
                    line: pos.0,
                    column: pos.1,
                });
            }
        }
        Ok(())
    }

    fn callable(&self, expr: &Expression) -> Result<Type, Finding> {
        let Expression::Function {
            generics,
            inputs,
            output,
            body,
        } = expr
        else {
            return Ok(Type::Unknown);
        };
        let mut inner = self.child();
        for item in generics {
            inner
                .generics
                .insert(item.name.clone(), item.bounds.clone());
        }
        for param in inputs {
            let kind = param
                .annotation
                .as_ref()
                .map_or(Type::Unknown, |spec| inner.from(spec));
            inner.locals.insert(param.name.clone(), kind);
        }
        let actual = inner.body(body)?;
        if let Some(spec) = output {
            let expected = inner.from(spec);
            if !compatible(&expected, &actual) {
                return Err(Finding {
                    code: Code::Coercion,
                    line: inputs.first().map_or(1, |p| p.line),
                    column: inputs.first().map_or(1, |p| p.column),
                });
            }
        }
        Ok(Type::Unknown)
    }
}
