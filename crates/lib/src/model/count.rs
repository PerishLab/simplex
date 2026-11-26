use crate::tree::{
    Body, Branch, Dispatch, Expression, Item, Loop, Outcome, Partition, Pattern, Statement,
};

pub(crate) struct Counter<'a> {
    target: &'a str,
}

impl<'a> Counter<'a> {
    pub(crate) fn new(target: &'a str) -> Self {
        Self { target }
    }

    pub(crate) fn items(&self, list: &[Item]) -> usize {
        list.iter()
            .map(|item| self.statement(&item.statement))
            .sum()
    }

    fn statement(&self, stmt: &Statement) -> usize {
        match stmt {
            Statement::Bind(binding) => self.expression(&binding.value),
            Statement::Conform(conf) => conf
                .methods
                .iter()
                .map(|entry| self.expression(&entry.value))
                .sum(),
            Statement::Assign { target, value, .. } => {
                self.expression(target) + self.expression(value)
            }
            Statement::For { source, body, .. } => {
                self.expression(source) + self.items(&body.items)
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.expression(expr),
            _ => 0,
        }
    }

    pub(crate) fn body(&self, body: &Body) -> usize {
        match body {
            Body::Block(block) => self.items(&block.items),
            Body::Switch(dispatch) => self.dispatch(dispatch),
            Body::Bucket(partition) => self.partition(partition),
            Body::Loop(kind) => self.repetition(kind),
        }
    }

    fn repetition(&self, kind: &Loop) -> usize {
        match kind {
            Loop::Block(block) => self.items(&block.items),
            Loop::Switch(dispatch) => self.dispatch(dispatch),
            Loop::Bucket(partition) => self.partition(partition),
        }
    }

    fn dispatch(&self, dispatch: &Dispatch) -> usize {
        self.expression(&dispatch.subject)
            + dispatch
                .arms
                .iter()
                .map(|arm| self.branch(arm))
                .sum::<usize>()
    }

    fn partition(&self, partition: &Partition) -> usize {
        let sub = partition
            .subject
            .as_ref()
            .map_or(0, |expr| self.expression(expr));
        sub + partition
            .arms
            .iter()
            .map(|arm| self.branch(arm))
            .sum::<usize>()
    }

    fn branch(&self, arm: &Branch) -> usize {
        let pat = match &arm.pattern {
            Pattern::Wildcard => 0,
            Pattern::Choices(list) => list.iter().map(|expr| self.expression(expr)).sum(),
            Pattern::Range { start, end } => self.expression(start) + self.expression(end),
            Pattern::Predicate(expr) => self.expression(expr),
        };
        let out = match &arm.outcome {
            Outcome::Value(expr) | Outcome::Break(Some(expr)) => self.expression(expr),
            Outcome::Break(None) | Outcome::Continue => 0,
            Outcome::Block(block) => self.items(&block.items),
        };
        pat + out
    }

    fn expression(&self, expr: &Expression) -> usize {
        match expr {
            Expression::Word { text, .. } => usize::from(text == self.target),
            Expression::Number(_) | Expression::Text(_) | Expression::Byte(_) => 0,
            Expression::Group(inner) | Expression::Try(inner) => self.expression(inner),
            Expression::List { items, .. } => items.iter().map(|item| self.expression(item)).sum(),
            Expression::Unary { operand, .. } => self.expression(operand),
            Expression::Borrow { target, .. } => self.expression(target),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.expression(base)
                    + self.expression(start)
                    + limit.as_deref().map_or(0, |end| self.expression(end))
            }
            Expression::Binary { left, right, .. } => {
                self.expression(left) + self.expression(right)
            }
            Expression::Member { base, .. } => self.expression(base),
            Expression::Call {
                callee, arguments, ..
            } => {
                self.expression(callee)
                    + arguments
                        .iter()
                        .map(|arg| self.expression(arg))
                        .sum::<usize>()
            }
            Expression::Record { fields, .. } => fields
                .iter()
                .map(|field| self.expression(&field.value))
                .sum(),
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            Expression::Function { body, .. } => self.body(body),
        }
    }
}
