pub mod emit;

use crate::layout::Catalog;
use crate::tree::{Case, Expression, Statement};

#[derive(Clone)]
pub struct Tracked {
    pub order: usize,
    pub symbol: String,
    pub borrowed: bool,
    pub kind: crate::Type,
    pub bit: u32,
    pub offset: u32,
}

#[derive(Clone)]
pub struct Plan {
    pub size: u32,
    pub items: Vec<Tracked>,
}

struct Candidate {
    order: usize,
    symbol: String,
    borrowed: bool,
    kind: crate::Type,
}

struct Scanner {
    order: usize,
    scopes: Vec<Vec<(usize, String, crate::Type)>>,
    found: Vec<Candidate>,
}

pub fn plan(
    inputs: &[(String, crate::Type)],
    body: &[Statement],
    catalog: &Catalog,
) -> Option<Plan> {
    let mut scanner = Scanner {
        order: 0,
        scopes: vec![Vec::new()],
        found: Vec::new(),
    };
    for (name, kind) in inputs {
        scanner.declare(name, kind);
    }
    scanner.statements(body);
    if scanner.found.is_empty() {
        return None;
    }
    scanner.found.sort_by_key(|item| item.order);
    let mut cursor = 8u32;
    let mut items = Vec::new();
    for (index, entry) in scanner.found.into_iter().take(63).enumerate() {
        let footprint = entry.kind.footprint(catalog);
        let align = footprint.align.max(8) as u32;
        cursor = (cursor + align - 1) & !(align - 1);
        let offset = cursor;
        cursor += footprint.size.max(8) as u32;
        items.push(Tracked {
            order: entry.order,
            symbol: entry.symbol,
            borrowed: entry.borrowed,
            kind: entry.kind,
            bit: index as u32,
            offset,
        });
    }
    Some(Plan {
        size: cursor.max(8),
        items,
    })
}

impl Scanner {
    fn declare(&mut self, name: &str, kind: &crate::Type) {
        let rank = self.order;
        self.order += 1;
        if let Some(top) = self.scopes.last_mut() {
            top.push((rank, name.to_owned(), kind.clone()));
        }
    }

    fn statements(&mut self, items: &[Statement]) {
        for item in items {
            match item {
                Statement::Bind {
                    name, kind, value, ..
                } => {
                    self.expression(value);
                    self.declare(name, kind);
                }
                Statement::Drop {
                    name,
                    symbol,
                    borrowed,
                } => self.record(name, symbol, *borrowed),
                Statement::Assign { value, .. }
                | Statement::Evaluate(value)
                | Statement::Result(value) => self.expression(value),
                Statement::Store { target, value } => {
                    self.expression(target);
                    self.expression(value);
                }
                Statement::Break(Some(expr)) => self.expression(expr),
                _ => {}
            }
        }
    }

    fn record(&mut self, name: &str, symbol: &str, borrowed: bool) {
        let found = self
            .scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|item| item.1 == name)
            .cloned();
        if let Some((order, _, kind)) = found
            && !matches!(kind, crate::Type::Pointer(_))
            && !self.found.iter().any(|item| item.order == order)
        {
            self.found.push(Candidate {
                order,
                symbol: symbol.to_owned(),
                borrowed,
                kind,
            });
        }
    }

    fn expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Unary { operand, .. }
            | Expression::Cast { value: operand, .. }
            | Expression::Borrow {
                target: operand, ..
            }
            | Expression::Load {
                pointer: operand, ..
            }
            | Expression::Field { base: operand, .. }
            | Expression::Length(operand)
            | Expression::Buffer { count: operand, .. }
            | Expression::Coerce { value: operand, .. }
            | Expression::Await { fiber: operand, .. }
            | Expression::All {
                fibers: operand, ..
            }
            | Expression::Race {
                fibers: operand, ..
            } => self.expression(operand),
            Expression::Binary { left, right, .. }
            | Expression::Index {
                base: left,
                index: right,
                ..
            } => {
                self.expression(left);
                self.expression(right);
            }
            Expression::Window {
                base, start, limit, ..
            } => {
                self.expression(base);
                self.expression(start);
                self.expression(limit);
            }
            Expression::Record { fields, .. } => {
                for (_, item) in fields {
                    self.expression(item);
                }
            }
            Expression::Variant { payloads, .. }
            | Expression::List {
                items: payloads, ..
            } => {
                for item in payloads {
                    self.expression(item);
                }
            }
            Expression::Call { arguments, .. } => {
                for item in arguments {
                    self.expression(item);
                }
            }
            Expression::Virtual {
                receiver,
                arguments,
                ..
            } => {
                self.expression(receiver);
                for item in arguments {
                    self.expression(item);
                }
            }
            Expression::Indirect {
                callee, arguments, ..
            } => {
                self.expression(callee);
                for item in arguments {
                    self.expression(item);
                }
            }
            Expression::Block { body, tail, .. } => self.scoped(body, tail.as_deref()),
            Expression::Branch {
                condition,
                then,
                otherwise,
                ..
            } => {
                self.expression(condition);
                self.expression(then);
                self.expression(otherwise);
            }
            Expression::Switch { subject, cases, .. } => self.switch(subject, cases),
            Expression::Loop { body, .. } => {
                self.scopes.push(Vec::new());
                self.statements(body);
                self.scopes.pop();
            }
            _ => {}
        }
    }

    fn scoped(&mut self, body: &[Statement], tail: Option<&Expression>) {
        self.scopes.push(Vec::new());
        self.statements(body);
        if let Some(expr) = tail {
            self.expression(expr);
        }
        self.scopes.pop();
    }

    fn switch(&mut self, subject: &Expression, cases: &[Case]) {
        self.expression(subject);
        for case in cases {
            self.scopes.push(Vec::new());
            for (name, field) in &case.bindings {
                self.declare(name, &field.kind);
            }
            self.expression(&case.body);
            self.scopes.pop();
        }
    }
}
