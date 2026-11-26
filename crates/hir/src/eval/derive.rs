use crate::eval::asset::zero;
use crate::layout::{Catalog, Field, Type};
use crate::tree::{Expression, Routine, Statement};

pub fn synthesize(
    target: (String, &Type),
    sig: (Vec<(String, Type)>, Type),
    catalog: &Catalog,
) -> Routine {
    let (symbol, subject) = target;
    let (inputs, output) = sig;
    let body = match (&output, inputs.as_slice()) {
        (Type::Bool, [left, right]) => vec![Statement::Result(equality(left, right, catalog))],
        (Type::Unsigned(8), [only]) => digest(only, catalog),
        (_, [only]) if output == *subject => vec![Statement::Result(clone(only, subject, catalog))],
        (Type::Void, _) => Vec::new(),
        _ => vec![Statement::Result(zero(&output, catalog))],
    };
    Routine {
        symbol,
        exported: false,
        asynchronous: false,
        inputs,
        output,
        body,
    }
}

fn access(param: &(String, Type)) -> Expression {
    let local = Expression::Local(param.0.clone(), param.1.clone());
    match &param.1 {
        Type::Pointer(inner) => Expression::Load {
            pointer: Box::new(local),
            kind: *inner.clone(),
        },
        _ => local,
    }
}

fn peel(kind: &Type) -> Type {
    match kind {
        Type::Pointer(inner) => *inner.clone(),
        other => other.clone(),
    }
}

fn project(base: Expression, slot: &Field) -> Expression {
    Expression::Field {
        base: Box::new(base),
        name: slot.name.clone(),
        offset: slot.offset,
        kind: slot.kind.clone(),
    }
}

fn equality(left: &(String, Type), right: &(String, Type), catalog: &Catalog) -> Expression {
    let lhs = access(left);
    let rhs = access(right);
    let Type::Record(name) = peel(&left.1) else {
        return same(lhs, rhs);
    };
    let Some(layout) = catalog.records.get(&name) else {
        return Expression::Bool(true);
    };
    let mut current = Expression::Bool(true);
    for slot in &layout.fields {
        let eq = compare(
            project(lhs.clone(), slot),
            project(rhs.clone(), slot),
            &slot.kind,
        );
        current = Expression::Binary {
            operator: String::from("&&"),
            left: Box::new(current),
            right: Box::new(eq),
            kind: Type::Bool,
        };
    }
    current
}

fn compare(lhs: Expression, rhs: Expression, kind: &Type) -> Expression {
    match kind {
        Type::Slice(_) => slices(lhs, rhs),
        _ => same(lhs, rhs),
    }
}

fn same(lhs: Expression, rhs: Expression) -> Expression {
    Expression::Binary {
        operator: String::from("=="),
        left: Box::new(lhs),
        right: Box::new(rhs),
        kind: Type::Bool,
    }
}

fn slices(lhs: Expression, rhs: Expression) -> Expression {
    let idx = String::from("index");
    let local = Expression::Local(idx.clone(), Type::Unsigned(8));
    let done = guard(
        Expression::Binary {
            operator: String::from(">="),
            left: Box::new(local.clone()),
            right: Box::new(Expression::Length(Box::new(lhs.clone()))),
            kind: Type::Bool,
        },
        true,
    );
    let diff = guard(
        Expression::Binary {
            operator: String::from("!="),
            left: Box::new(element(lhs.clone(), local.clone())),
            right: Box::new(element(rhs.clone(), local.clone())),
            kind: Type::Bool,
        },
        false,
    );
    let step = Statement::Assign {
        name: idx.clone(),
        value: Expression::Binary {
            operator: String::from("+"),
            left: Box::new(local),
            right: Box::new(Expression::Int(1, Type::Unsigned(8))),
            kind: Type::Unsigned(8),
        },
    };
    Expression::Branch {
        condition: Box::new(same(
            Expression::Length(Box::new(lhs)),
            Expression::Length(Box::new(rhs)),
        )),
        then: Box::new(Expression::Block {
            body: vec![Statement::Bind {
                name: idx,
                mutable: true,
                kind: Type::Unsigned(8),
                value: Expression::Int(0, Type::Unsigned(8)),
            }],
            tail: Some(Box::new(Expression::Loop {
                body: vec![done, diff, step],
                kind: Type::Bool,
            })),
            kind: Type::Bool,
        }),
        otherwise: Box::new(Expression::Bool(false)),
        kind: Type::Bool,
    }
}

fn element(slice: Expression, index: Expression) -> Expression {
    Expression::Index {
        base: Box::new(slice),
        index: Box::new(index),
        kind: Type::Unsigned(1),
    }
}

fn guard(cond: Expression, result: bool) -> Statement {
    Statement::Evaluate(Expression::Branch {
        condition: Box::new(cond),
        then: Box::new(Expression::Block {
            body: vec![Statement::Break(Some(Expression::Bool(result)))],
            tail: None,
            kind: Type::Bool,
        }),
        otherwise: Box::new(Expression::Void),
        kind: Type::Void,
    })
}

fn digest(only: &(String, Type), catalog: &Catalog) -> Vec<Statement> {
    let acc = String::from("acc");
    let mut stmts = vec![Statement::Bind {
        name: acc.clone(),
        mutable: true,
        kind: Type::Unsigned(8),
        value: Expression::Int(14_695_981_039_346_656_037u64 as i128, Type::Unsigned(8)),
    }];
    let base = access(only);
    if let Type::Record(name) = peel(&only.1)
        && let Some(layout) = catalog.records.get(&name)
    {
        for slot in &layout.fields {
            let val = match &slot.kind {
                Type::Slice(_) => Expression::Length(Box::new(project(base.clone(), slot))),
                _ => Expression::Cast {
                    value: Box::new(project(base.clone(), slot)),
                    kind: Type::Unsigned(8),
                },
            };
            stmts.push(mix(&acc, val));
        }
    }
    stmts.push(Statement::Result(Expression::Local(acc, Type::Unsigned(8))));
    stmts
}

fn mix(acc: &str, val: Expression) -> Statement {
    let xored = Expression::Binary {
        operator: String::from("^"),
        left: Box::new(Expression::Local(acc.to_owned(), Type::Unsigned(8))),
        right: Box::new(val),
        kind: Type::Unsigned(8),
    };
    Statement::Assign {
        name: acc.to_owned(),
        value: Expression::Binary {
            operator: String::from("*"),
            left: Box::new(xored),
            right: Box::new(Expression::Int(1_099_511_628_211, Type::Unsigned(8))),
            kind: Type::Unsigned(8),
        },
    }
}

fn clone(only: &(String, Type), subject: &Type, catalog: &Catalog) -> Expression {
    let base = access(only);
    let Type::Record(name) = subject else {
        return base;
    };
    let fields = catalog
        .records
        .get(name)
        .map(|l| {
            l.fields
                .iter()
                .map(|f| (f.name.clone(), project(base.clone(), f)))
                .collect()
        })
        .unwrap_or_default();
    Expression::Record {
        name: name.clone(),
        fields,
        kind: subject.clone(),
    }
}
