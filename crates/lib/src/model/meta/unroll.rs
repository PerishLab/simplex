use super::eval::{Machine, Value};
use crate::code::{Code, Finding};
use crate::model::Table;
use crate::model::borrow::root;
use crate::tree::{Body, Expression, Item, Spec, Statement};
use std::collections::BTreeMap;

impl Table {
    pub(super) fn intrinsic(
        &self,
        callee: &Expression,
        generics: &[Spec],
        pos: (usize, usize),
    ) -> Result<(), Finding> {
        let Expression::Word { text, .. } = callee else {
            return Ok(());
        };
        if !matches!(text.as_str(), "fields" | "variants" | "size") {
            return Ok(());
        }
        let Some(spec) = generics.first() else {
            return Err(Finding {
                code: Code::Target,
                line: pos.0,
                column: pos.1,
            });
        };
        if self.unresolved(spec) {
            return Ok(());
        }
        let valid = match text.as_str() {
            "fields" => self
                .nominal(spec)
                .is_some_and(|n| self.structs.contains_key(&n)),
            "variants" => self
                .nominal(spec)
                .is_some_and(|n| self.enums.contains_key(&n)),
            _ => super::size(self, spec).is_some(),
        };
        if !valid {
            return Err(Finding {
                code: Code::Target,
                line: pos.0,
                column: pos.1,
            });
        }
        Ok(())
    }

    fn unresolved(&self, spec: &Spec) -> bool {
        let Spec::Named {
            module: None, name, ..
        } = spec
        else {
            return false;
        };
        if super::size(self, spec).is_some() {
            return false;
        }
        let resolved = self.resolve(name);
        !self.structs.contains_key(&resolved) && !self.enums.contains_key(&resolved)
    }

    pub(super) fn direct(&self, source: &Expression, pos: (usize, usize)) -> Result<(), Finding> {
        if let Expression::Call {
            callee, generics, ..
        } = source
        {
            return self.intrinsic(callee, generics, pos);
        }
        Ok(())
    }

    pub(super) fn assertions(
        &self,
        env: &BTreeMap<String, Value>,
        body: &Body,
    ) -> Result<(), Finding> {
        let Body::Block(block) = body else {
            return Ok(());
        };
        for item in &block.items {
            if let Statement::Evaluate(Expression::Meta(meta)) = &item.statement {
                Machine::new(self, env.clone()).block(meta)?;
            }
        }
        Ok(())
    }

    pub(super) fn specialize(
        &self,
        locals: &BTreeMap<String, Option<String>>,
        call: &Expression,
    ) -> Result<(), Finding> {
        let Expression::Call {
            callee,
            arguments,
            line,
            column,
            ..
        } = call
        else {
            return Ok(());
        };
        let Expression::Word { text: name, .. } = callee.as_ref() else {
            return Ok(());
        };
        let pos = (*line, *column);
        let Some(sig) = self.functions.get(name) else {
            return Ok(());
        };
        if sig.generics.is_empty() {
            return Ok(());
        }
        let Some((_, Body::Block(block))) = self.bodies.get(name) else {
            return Ok(());
        };
        if !block
            .items
            .iter()
            .any(|i| matches!(i.statement, Statement::For { constant: true, .. }))
        {
            return Ok(());
        }
        let Some(concrete) = arguments
            .first()
            .and_then(root)
            .and_then(|r| locals.get(&r))
            .and_then(Clone::clone)
        else {
            return Ok(());
        };
        let Some(structure) = self.structs.get(&concrete) else {
            return Err(Finding {
                code: Code::Target,
                line: pos.0,
                column: pos.1,
            });
        };
        if converts(&block.items) && structure.fields.iter().any(|f| !convertible(&f.spec)) {
            return Err(Finding {
                code: Code::Coercion,
                line: pos.0,
                column: pos.1,
            });
        }
        Ok(())
    }
}

fn convertible(spec: &Spec) -> bool {
    matches!(
        spec,
        Spec::Named { module: None, name, .. }
            if matches!(
                name.as_str(),
                "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" | "f32" | "f64"
            )
    )
}

fn converts(items: &[Item]) -> bool {
    items.iter().any(|item| match &item.statement {
        Statement::For {
            constant: true,
            body,
            ..
        } => body.items.iter().any(|sub| mentions(&sub.statement)),
        _ => false,
    })
}

fn mentions(stmt: &Statement) -> bool {
    match stmt {
        Statement::Assign { value, .. } | Statement::Evaluate(value) => expr(value),
        Statement::Bind(b) => expr(&b.value),
        _ => false,
    }
}

fn expr(e: &Expression) -> bool {
    match e {
        Expression::Group(i) | Expression::Try(i) => expr(i),
        Expression::Binary { left, right, .. } => expr(left) || expr(right),
        Expression::Call {
            callee, arguments, ..
        } => expr(callee) || arguments.iter().any(expr),
        Expression::Member { base, field, .. } => {
            matches!(field.as_str(), "into" | "as") || expr(base)
        }
        _ => false,
    }
}
