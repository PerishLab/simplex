use super::Table;
use super::flow;
use crate::code::{Code, Finding};
use crate::tree::{
    Body, Branch, Enumeration, Expression, Item, Outcome, Parameter, Program, Spec, Statement,
    Structure,
};
use std::collections::BTreeSet;

impl Table {
    pub(crate) fn verify(&self, program: &Program) -> Result<(), Finding> {
        storage(&program.items)?;
        self.replicas()?;
        self.inspect(&program.items)
    }

    fn replicas(&self) -> Result<(), Finding> {
        for claim in &self.claims {
            if self.resolve(&claim.contract) != "Copy" {
                continue;
            }
            let subject = self.resolve(&claim.subject);
            if !self.duplicable(&subject) {
                return Err(Finding {
                    code: Code::Replica,
                    line: claim.line,
                    column: claim.column,
                });
            }
        }
        Ok(())
    }

    fn duplicable(&self, subject: &str) -> bool {
        if let Some(structure) = self.structs.get(subject) {
            return structure
                .fields
                .iter()
                .all(|slot| self.copyable(&slot.spec));
        }
        if let Some(enumeration) = self.enums.get(subject) {
            return enumeration
                .variants
                .iter()
                .flat_map(|variant| &variant.payloads)
                .all(|spec| self.copyable(spec));
        }
        false
    }

    pub(crate) fn copyable(&self, spec: &Spec) -> bool {
        match spec {
            Spec::Borrow { mutable, .. } => !*mutable,
            Spec::Callable { .. } => true,
            Spec::Slice(_)
            | Spec::Named {
                module: Some(_), ..
            } => false,
            Spec::Named {
                module: None, name, ..
            } => {
                if primitive(name) {
                    return true;
                }
                let resolved = self.resolve(name);
                self.copies.contains(&resolved)
                    || (!self.structs.contains_key(&resolved)
                        && !self.enums.contains_key(&resolved))
            }
        }
    }

    fn inspect(&self, list: &[Item]) -> Result<(), Finding> {
        for item in list {
            self.item(item)?;
        }
        Ok(())
    }

    fn item(&self, item: &Item) -> Result<(), Finding> {
        let pos = (item.line, item.column);
        match &item.statement {
            Statement::Bind(binding) => self.expression(&binding.value, pos),
            Statement::Conform(conf) => {
                for entry in &conf.methods {
                    self.expression(&entry.value, pos)?;
                }
                Ok(())
            }
            Statement::Evaluate(expr) => self.expression(expr, pos),
            _ => Ok(()),
        }
    }

    fn expression(&self, expr: &Expression, pos: (usize, usize)) -> Result<(), Finding> {
        let Expression::Function {
            asynchronous,
            inputs,
            output,
            body,
            ..
        } = expr
        else {
            return Ok(());
        };
        if output.as_ref().is_some_and(contains) {
            escape(inputs, body, pos)?;
        }
        flow::verify(self, *asynchronous, inputs, body)
    }
}

fn storage(list: &[Item]) -> Result<(), Finding> {
    for item in list {
        check(&item.statement, (item.line, item.column))?;
    }
    Ok(())
}

fn check(stmt: &Statement, pos: (usize, usize)) -> Result<(), Finding> {
    let invalid = match stmt {
        Statement::Struct(Structure { fields, .. }) => fields.iter().any(|s| contains(&s.spec)),
        Statement::Enum(Enumeration { variants, .. }) => {
            variants.iter().flat_map(|v| &v.payloads).any(contains)
        }
        _ => false,
    };
    if invalid {
        return Err(Finding {
            code: Code::Storage,
            line: pos.0,
            column: pos.1,
        });
    }
    Ok(())
}

fn primitive(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
    )
}

pub(crate) fn contains(spec: &Spec) -> bool {
    match spec {
        Spec::Borrow { .. } => true,
        Spec::Slice(inner) => contains(inner),
        Spec::Named { arguments, .. } => arguments.iter().any(contains),
        Spec::Callable { inputs, output, .. } => {
            inputs.iter().any(contains) || output.as_deref().is_some_and(contains)
        }
    }
}

fn escape(inputs: &[Parameter], body: &Body, pos: (usize, usize)) -> Result<(), Finding> {
    let sources: BTreeSet<String> = inputs
        .iter()
        .filter(|param| param.annotation.as_ref().is_some_and(contains))
        .map(|param| param.name.clone())
        .collect();
    if sources.is_empty() {
        return Err(Finding {
            code: Code::Escape,
            line: pos.0,
            column: pos.1,
        });
    }
    returned(body, &sources)
}

fn returned(body: &Body, sources: &BTreeSet<String>) -> Result<(), Finding> {
    match body {
        Body::Block(block) => tail(&block.items, sources),
        Body::Switch(dispatch) => branches(&dispatch.arms, sources),
        Body::Bucket(partition) => branches(&partition.arms, sources),
        Body::Loop(_) => Ok(()),
    }
}

fn tail(items: &[Item], sources: &BTreeSet<String>) -> Result<(), Finding> {
    if let Some(last) = items.last()
        && let Statement::Evaluate(expr) = &last.statement
    {
        rooted(expr, sources, (last.line, last.column))?;
    }
    Ok(())
}

fn branches(arms: &[Branch], sources: &BTreeSet<String>) -> Result<(), Finding> {
    for arm in arms {
        outcome(&arm.outcome, sources, (arm.line, arm.column))?;
    }
    Ok(())
}

fn outcome(out: &Outcome, sources: &BTreeSet<String>, pos: (usize, usize)) -> Result<(), Finding> {
    match out {
        Outcome::Value(expr) | Outcome::Break(Some(expr)) => rooted(expr, sources, pos),
        Outcome::Block(block) => tail(&block.items, sources),
        Outcome::Break(None) | Outcome::Continue => Ok(()),
    }
}

fn rooted(
    expr: &Expression,
    sources: &BTreeSet<String>,
    pos: (usize, usize),
) -> Result<(), Finding> {
    if let Some(origin) = source(expr)
        && !sources.contains(&origin)
    {
        return Err(Finding {
            code: Code::Escape,
            line: pos.0,
            column: pos.1,
        });
    }
    Ok(())
}

pub(crate) fn source(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Word { text, .. } => Some(text.clone()),
        Expression::Group(inner) | Expression::Try(inner) => source(inner),
        Expression::Borrow { target, .. } => root(target),
        Expression::Member { base, .. } | Expression::Index { base, .. } => root(base),
        Expression::Call { arguments, .. } => arguments.iter().find_map(source),
        _ => None,
    }
}

pub(crate) fn root(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Word { text, .. } => Some(text.clone()),
        Expression::Group(inner)
        | Expression::Try(inner)
        | Expression::Borrow { target: inner, .. } => root(inner),
        Expression::Member { base, .. } | Expression::Index { base, .. } => root(base),
        _ => None,
    }
}
