use crate::code::{Code, Finding};
use crate::model::Signature;
use crate::model::check::{Scope, Type, compatible};
use crate::tree::{Expression, Generic, Spec};

impl<'a> Scope<'a> {
    pub(super) fn projected(&self, owner: Type, field: &str) -> Result<Type, Finding> {
        let peeled = match owner {
            Type::Borrow { inner, .. } => *inner,
            other => other,
        };
        if let Type::Nominal(name) = &peeled {
            if name == "Origin" {
                return Ok(match field {
                    "line" | "column" => Type::Scalar("u64".into()),
                    "package" | "module" | "file" => Type::Scalar("str".into()),
                    _ => Type::Unknown,
                });
            }
            if let Some(spec) = self.table.field(name, field) {
                return Ok(self.from(&spec));
            }
            if let Some(sig) = self.table.method(name, field) {
                let out = sig.output.as_ref().map_or(Type::Unknown, |s| self.from(s));
                return Ok(wrap(sig.asynchronous, out));
            }
            if let Some(contract) = self.table.interfaces.get(name)
                && let Some(req) = contract.methods.iter().find(|m| m.name == field)
            {
                let out = req.output.as_ref().map_or(Type::Unknown, |s| self.from(s));
                return Ok(wrap(req.asynchronous, out));
            }
        }
        if let Type::Generic(param) = &peeled
            && let Some(bounds) = self.generics.get(param)
        {
            return Ok(self.bounded(bounds, field));
        }
        Ok(Type::Unknown)
    }

    fn bounded(&self, bounds: &[String], field: &str) -> Type {
        for bound in bounds {
            let Some(contract) = self.table.interfaces.get(&self.table.resolve(bound)) else {
                continue;
            };
            let Some(req) = contract.methods.iter().find(|m| m.name == field) else {
                continue;
            };
            let out = req.output.as_ref().map_or(Type::Unknown, |s| self.from(s));
            return wrap(req.asynchronous, out);
        }
        Type::Unknown
    }

    pub(super) fn call(&mut self, expr: &Expression) -> Result<Type, Finding> {
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
        if let Some(mapped) = self.mapped(callee, arguments)? {
            return Ok(mapped);
        }
        if let Expression::Word { text, .. } = callee.as_ref()
            && let Some(kind) = self.intrinsic(text, arguments)?
        {
            return Ok(kind);
        }
        if matches!(callee.as_ref(), Expression::Member { field, .. } if field == "len") {
            self.infer(callee)?;
            return Ok(Type::Scalar("usize".into()));
        }
        let mut actuals = Vec::new();
        for arg in arguments {
            actuals.push(self.infer(arg)?);
        }
        if let Expression::Word { text, .. } = callee.as_ref()
            && let Some(sig) = self.table.functions.get(text)
        {
            self.bounds(sig, &actuals, (*line, *column))?;
            let out = sig.output.as_ref().map_or(Type::Unknown, |s| self.from(s));
            return Ok(wrap(sig.asynchronous, out));
        }
        self.infer(callee)
    }

    fn intrinsic(&mut self, name: &str, arguments: &[Expression]) -> Result<Option<Type>, Finding> {
        match name {
            "size" => Ok(Some(Type::Scalar("usize".into()))),
            "has" => Ok(Some(Type::Scalar("bool".into()))),
            "caller" => Ok(Some(Type::Nominal("Origin".into()))),
            "embed" => {
                for arg in arguments {
                    self.infer(arg)?;
                }
                Ok(Some(Type::Borrow {
                    mutable: false,
                    inner: Box::new(Type::Slice(Box::new(Type::Scalar("u8".into())))),
                }))
            }
            _ => Ok(None),
        }
    }

    fn mapped(
        &mut self,
        callee: &Expression,
        args: &[Expression],
    ) -> Result<Option<Type>, Finding> {
        let Expression::Member { base, field, .. } = callee else {
            return Ok(None);
        };
        if field != "map" {
            return Ok(None);
        }
        let [Expression::Function { inputs, body, .. }] = args else {
            return Ok(None);
        };
        let container = self.infer(base)?;
        let item = element(container);
        let mut inner = self.child();
        if let Some(first) = inputs.first() {
            inner.locals.insert(first.name.clone(), item);
        }
        Ok(Some(Type::List(Box::new(inner.body(body)?))))
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
        let Some(concrete) = nominal(actual) else {
            return Ok(());
        };
        if !self.table.structs.contains_key(concrete) && !self.table.enums.contains_key(concrete) {
            return Ok(());
        }
        for bound in &clause.bounds {
            if !self
                .table
                .satisfied
                .contains(&(concrete.to_owned(), self.table.resolve(bound)))
            {
                return Err(Finding {
                    code: Code::Conformance,
                    line: pos.0,
                    column: pos.1,
                });
            }
        }
        Ok(())
    }

    pub(super) fn callable(&self, expr: &Expression) -> Result<Type, Finding> {
        let Expression::Function {
            generics,
            inputs,
            output,
            body,
            ..
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

fn element(container: Type) -> Type {
    match container {
        Type::Borrow { inner, .. } => match *inner {
            Type::Slice(elem) => Type::Borrow {
                mutable: false,
                inner: elem,
            },
            _ => Type::Unknown,
        },
        Type::List(elem) => *elem,
        _ => Type::Unknown,
    }
}

fn nominal(actual: &Type) -> Option<&str> {
    match actual {
        Type::Nominal(name) => Some(name),
        Type::Borrow { inner, .. } => nominal(inner),
        _ => None,
    }
}

fn wrap(asynchronous: bool, kind: Type) -> Type {
    if asynchronous {
        Type::Fiber(Box::new(kind))
    } else {
        kind
    }
}
