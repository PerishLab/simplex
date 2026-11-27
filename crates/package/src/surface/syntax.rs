use super::loader::Unit;
use simplex::tree::{Body, Expression, Generic, Outcome, Parameter, Spec, Statement};

impl Unit {
    pub(crate) fn parameters(&self, list: &[Parameter]) -> String {
        let parts: Vec<String> = list.iter().map(|param| self.parameter(param)).collect();
        parts.join(",")
    }

    fn parameter(&self, param: &Parameter) -> String {
        let kind = match (&param.annotation, param.constant) {
            (Some(ann), _) => self.spec(ann),
            (None, true) => String::from("comptime"),
            (None, false) => String::from("i64"),
        };
        match param.constant {
            true => format!("const:{kind}"),
            false => kind,
        }
    }

    pub(crate) fn spec(&self, node: &Spec) -> String {
        match node {
            Spec::Named {
                module,
                name,
                arguments,
            } => self.named(module.as_deref(), name, arguments),
            Spec::Borrow {
                mutable: true,
                target,
            } => format!("&var {}", self.spec(target)),
            Spec::Borrow {
                mutable: false,
                target,
            } => format!("&{}", self.spec(target)),
            Spec::Slice(inner) => format!("[{}]", self.spec(inner)),
            Spec::Callable {
                asynchronous,
                inputs,
                output,
            } => self.callable(*asynchronous, inputs, output.as_deref()),
        }
    }

    fn callable(&self, asynchronous: bool, inputs: &[Spec], output: Option<&Spec>) -> String {
        let head = if asynchronous { "async fn" } else { "fn" };
        let params: Vec<String> = inputs.iter().map(|item| self.spec(item)).collect();
        match output {
            Some(out) => format!("{head}({})->{}", params.join(","), self.spec(out)),
            None => format!("{head}({})", params.join(",")),
        }
    }

    fn named(&self, owner: Option<&str>, name: &str, args: &[Spec]) -> String {
        let base = self.qualify(owner, name);
        if args.is_empty() {
            return base;
        }
        let rendered: Vec<String> = args.iter().map(|item| self.spec(item)).collect();
        format!("{base}<{}>", rendered.join(","))
    }

    fn qualify(&self, owner: Option<&str>, name: &str) -> String {
        let Some(alias) = owner else {
            return name.to_owned();
        };
        if let Some(local) = self.locals.get(alias) {
            return match local.is_empty() {
                true => name.to_owned(),
                false => format!("{local}.{name}"),
            };
        }
        if let Some(ext) = self.external.get(alias) {
            return format!("ext:{ext}.{name}");
        }
        format!("{alias}.{name}")
    }

    pub(crate) fn inferred(&self, body: &Body) -> String {
        match body {
            Body::Block(block) => match block.items.last().map(|item| &item.statement) {
                Some(Statement::Evaluate(expr)) => self.literal(expr),
                _ => String::from("void"),
            },
            Body::Switch(dispatch) => dispatch
                .arms
                .first()
                .map_or_else(|| String::from("void"), |arm| self.outcome(&arm.outcome)),
            Body::Bucket(partition) => partition
                .arms
                .first()
                .map_or_else(|| String::from("void"), |arm| self.outcome(&arm.outcome)),
            Body::Loop(_) => String::from("void"),
        }
    }

    fn outcome(&self, out: &Outcome) -> String {
        match out {
            Outcome::Value(expr) | Outcome::Break(Some(expr)) => self.literal(expr),
            Outcome::Block(block) => match block.items.last().map(|item| &item.statement) {
                Some(Statement::Evaluate(expr)) => self.literal(expr),
                _ => String::from("void"),
            },
            Outcome::Break(None) | Outcome::Continue => String::from("void"),
        }
    }

    pub(crate) fn literal(&self, expr: &Expression) -> String {
        match expr {
            Expression::Number(text) => match text.contains('.') {
                true => String::from("f64"),
                false => String::from("i64"),
            },
            Expression::Text(_) => String::from("[u8]"),
            Expression::Byte(_) => String::from("u8"),
            Expression::Word { text, .. } if text == "true" || text == "false" => {
                String::from("bool")
            }
            Expression::Group(inner) => self.literal(inner),
            Expression::Unary { operator, operand } => match operator.as_str() {
                "!" => String::from("bool"),
                _ => self.literal(operand),
            },
            Expression::Borrow {
                mutable: true,
                target,
                ..
            } => format!("&var {}", self.literal(target)),
            Expression::Borrow {
                mutable: false,
                target,
                ..
            } => format!("&{}", self.literal(target)),
            Expression::Record { module, name, .. } => self.named(module.as_deref(), name, &[]),
            Expression::Call { callee, .. } if matches!(callee.as_ref(), Expression::Word { text, .. } if text == "embed") => {
                String::from("[u8]")
            }
            _ => String::from("i64"),
        }
    }
}

pub(crate) fn generics(list: &[Generic]) -> String {
    let parts: Vec<String> = list
        .iter()
        .map(|item| match item.bounds.is_empty() {
            true => item.name.clone(),
            false => format!("{}:{}", item.name, item.bounds.join("+")),
        })
        .collect();
    parts.join(",")
}

pub(crate) fn prefix(id: &str) -> String {
    match id.is_empty() {
        true => String::new(),
        false => format!("{id}::"),
    }
}

pub(crate) fn trim(raw: &str) -> &str {
    raw.trim_matches('"')
}
