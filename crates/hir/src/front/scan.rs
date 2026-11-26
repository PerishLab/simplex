use crate::front::world::World;
use simplex::tree::{
    Body, Branch, Enumeration, Expression, Item, Loop, Outcome, Pattern, Spec, Statement, Variant,
};
use std::collections::BTreeMap;

type Arms = BTreeMap<String, Vec<(String, usize)>>;

pub fn placeholders(world: &mut World, stem: &str, items: &[Item]) {
    let mut arms = Arms::new();
    for item in items {
        stmt(&item.statement, &mut arms);
    }
    for (choice, found) in arms {
        let key = (stem.to_owned(), choice.clone());
        if world.enums.contains_key(&key)
            || world.structs.contains_key(&key)
            || world.aliases.contains_key(&key)
        {
            continue;
        }
        world.enums.insert(key, synthesize(choice, found));
    }
}

fn synthesize(choice: String, found: Vec<(String, usize)>) -> Enumeration {
    let variants = found
        .into_iter()
        .map(|(arm, count)| Variant {
            decorators: Vec::new(),
            name: arm,
            payloads: (0..count)
                .map(|_| Spec::Named {
                    module: None,
                    name: String::from("u64"),
                    arguments: Vec::new(),
                })
                .collect(),
        })
        .collect();
    Enumeration {
        decorators: Vec::new(),
        public: false,
        dynamic: false,
        name: choice,
        generics: Vec::new(),
        variants,
    }
}

fn stmt(node: &Statement, arms: &mut Arms) {
    match node {
        Statement::Bind(b) => expr(&b.value, arms),
        Statement::Assign { target, value, .. } => {
            expr(target, arms);
            expr(value, arms);
        }
        Statement::For { source, body, .. } => {
            expr(source, arms);
            list(&body.items, arms);
        }
        Statement::Loop(kind) => looped(kind, arms),
        Statement::Evaluate(e) => expr(e, arms),
        _ => {}
    }
}

fn list(items: &[Item], arms: &mut Arms) {
    for sub in items {
        stmt(&sub.statement, arms);
    }
}

fn looped(kind: &Loop, arms: &mut Arms) {
    match kind {
        Loop::Block(b) => list(&b.items, arms),
        Loop::Switch(d) => {
            expr(&d.subject, arms);
            branches(&d.arms, arms);
        }
        Loop::Bucket(p) => {
            if let Some(s) = &p.subject {
                expr(s, arms);
            }
            branches(&p.arms, arms);
        }
    }
}

fn branches(items: &[Branch], arms: &mut Arms) {
    for arm in items {
        if let Pattern::Choices(choices) = &arm.pattern {
            for item in choices {
                expr(item, arms);
            }
        }
        match &arm.outcome {
            Outcome::Value(e) | Outcome::Break(Some(e)) => expr(e, arms),
            Outcome::Block(b) => list(&b.items, arms),
            _ => {}
        }
    }
}

fn expr(node: &Expression, arms: &mut Arms) {
    match node {
        Expression::Member { base, field, .. } => member(base, field, arms),
        Expression::Call {
            callee, arguments, ..
        } => call(callee, arguments, arms),
        Expression::Group(inner) | Expression::Try(inner) => expr(inner, arms),
        Expression::Unary { operand, .. }
        | Expression::Borrow {
            target: operand, ..
        } => {
            expr(operand, arms);
        }
        Expression::Binary { left, right, .. } => {
            expr(left, arms);
            expr(right, arms);
        }
        Expression::Switch(d) => looped(&Loop::Switch(d.clone()), arms),
        Expression::Bucket(p) => looped(&Loop::Bucket(p.clone()), arms),
        Expression::Loop(k) => looped(k, arms),
        Expression::Function { body, .. } => callable(body, arms),
        _ => {}
    }
}

fn callable(body: &Body, arms: &mut Arms) {
    match body {
        Body::Block(b) => list(&b.items, arms),
        Body::Switch(d) => looped(&Loop::Switch(d.clone()), arms),
        Body::Bucket(p) => looped(&Loop::Bucket(p.clone()), arms),
        Body::Loop(k) => looped(k, arms),
    }
}

fn member(base: &Expression, field: &str, arms: &mut Arms) {
    if let Expression::Word { text, .. } = base
        && text.chars().next().is_some_and(|c| c.is_ascii_uppercase())
    {
        record(arms, text, field, 0);
    }
    expr(base, arms);
}

fn call(callee: &Expression, arguments: &[Expression], arms: &mut Arms) {
    let tagged = match callee {
        Expression::Member { base, field, .. } => match base.as_ref() {
            Expression::Word { text, .. }
                if text.chars().next().is_some_and(|c| c.is_ascii_uppercase()) =>
            {
                record(arms, text, field, arguments.len());
                true
            }
            _ => false,
        },
        _ => false,
    };
    if !tagged {
        expr(callee, arms);
    }
    for arg in arguments {
        expr(arg, arms);
    }
}

fn record(arms: &mut Arms, choice: &str, arm: &str, count: usize) {
    let list = arms.entry(choice.to_owned()).or_default();
    match list.iter_mut().find(|(name, _)| name == arm) {
        Some(found) => found.1 = found.1.max(count),
        None => list.push((arm.to_owned(), count)),
    }
}
