use crate::code::{Code, Finding};
use crate::tree::{
    Block, Body, Branch, Dispatch, Expression, Item, Loop, Outcome, Partition, Pattern, Program,
    Statement,
};

const LIMIT: usize = 4;

pub fn check(program: &Program) -> Result<(), Finding> {
    Inspector.items(&program.items, 0)
}

struct Inspector;

impl Inspector {
    fn items(&self, list: &[Item], depth: usize) -> Result<(), Finding> {
        for item in list {
            self.statement(&item.statement, depth)?;
        }
        if let [.., prev, tail] = list
            && let Statement::Bind(binding) = &prev.statement
            && let Statement::Evaluate(Expression::Word { text, .. }) = &tail.statement
            && binding.name == *text
        {
            return Err(Finding {
                code: Code::Binding,
                line: prev.line,
                column: prev.column,
            });
        }
        Ok(())
    }

    fn statement(&self, stmt: &Statement, depth: usize) -> Result<(), Finding> {
        match stmt {
            Statement::Bind(binding) => self.expression(&binding.value, depth),
            Statement::Conform(item) => {
                for entry in &item.methods {
                    self.expression(&entry.value, depth + 1)?;
                }
                Ok(())
            }
            Statement::Struct(_)
            | Statement::Enum(_)
            | Statement::Interface(_)
            | Statement::Alias(_)
            | Statement::Import(_)
            | Statement::Extern(_) => Ok(()),
            Statement::Assign { target, value, .. } => {
                self.expression(target, depth)?;
                self.expression(value, depth)
            }
            Statement::For { source, body, .. } => {
                self.expression(source, depth)?;
                self.block(body, depth + 1)
            }
            Statement::Loop(kind) => self.repetition(kind, depth),
            Statement::Evaluate(expr) => self.expression(expr, depth),
        }
    }

    fn block(&self, block: &Block, depth: usize) -> Result<(), Finding> {
        if depth > LIMIT {
            return Err(Finding {
                code: Code::Depth,
                line: block.line,
                column: block.column,
            });
        }
        self.items(&block.items, depth)
    }

    fn repetition(&self, kind: &Loop, depth: usize) -> Result<(), Finding> {
        match kind {
            Loop::Block(block) => self.block(block, depth + 1),
            Loop::Switch(dispatch) => self.dispatch(dispatch, depth),
            Loop::Bucket(partition) => self.partition(partition, depth),
        }
    }

    fn dispatch(&self, dispatch: &Dispatch, depth: usize) -> Result<(), Finding> {
        if depth + 1 > LIMIT {
            return Err(Finding {
                code: Code::Depth,
                line: dispatch.line,
                column: dispatch.column,
            });
        }
        self.expression(&dispatch.subject, depth)?;
        for (index, arm) in dispatch.arms.iter().enumerate() {
            if matches!(arm.pattern, Pattern::Range { .. }) {
                return Err(Finding {
                    code: Code::Switch,
                    line: arm.line,
                    column: arm.column,
                });
            }
            self.branch(arm, index + 1 == dispatch.arms.len(), depth + 1)?;
        }
        Ok(())
    }

    fn partition(&self, partition: &Partition, depth: usize) -> Result<(), Finding> {
        if depth + 1 > LIMIT {
            return Err(Finding {
                code: Code::Depth,
                line: partition.line,
                column: partition.column,
            });
        }
        if let Some(subject) = &partition.subject {
            self.expression(subject, depth)?;
        }
        for (index, arm) in partition.arms.iter().enumerate() {
            self.branch(arm, index + 1 == partition.arms.len(), depth + 1)?;
        }
        if !matches!(
            partition.arms.last(),
            Some(Branch {
                pattern: Pattern::Wildcard,
                ..
            })
        ) {
            return Err(Finding {
                code: Code::Bucket,
                line: partition.line,
                column: partition.column,
            });
        }
        Ok(())
    }

    fn branch(&self, arm: &Branch, tail: bool, depth: usize) -> Result<(), Finding> {
        if matches!(arm.pattern, Pattern::Wildcard) && !tail {
            return Err(Finding {
                code: Code::Arm,
                line: arm.line,
                column: arm.column,
            });
        }
        self.pattern(&arm.pattern, depth)?;
        self.outcome(&arm.outcome, depth)
    }

    fn pattern(&self, pat: &Pattern, depth: usize) -> Result<(), Finding> {
        match pat {
            Pattern::Wildcard => Ok(()),
            Pattern::Choices(list) => {
                for expr in list {
                    self.expression(expr, depth)?;
                }
                Ok(())
            }
            Pattern::Range { start, end } => {
                self.expression(start, depth)?;
                self.expression(end, depth)
            }
            Pattern::Predicate(expr) => self.expression(expr, depth),
        }
    }

    fn outcome(&self, out: &Outcome, depth: usize) -> Result<(), Finding> {
        match out {
            Outcome::Value(expr) | Outcome::Break(Some(expr)) => self.expression(expr, depth),
            Outcome::Break(None) | Outcome::Continue => Ok(()),
            Outcome::Block(block) => self.block(block, depth + 1),
        }
    }

    fn expression(&self, expr: &Expression, depth: usize) -> Result<(), Finding> {
        match expr {
            Expression::Word { .. }
            | Expression::Number(_)
            | Expression::Text(_)
            | Expression::Byte(_) => Ok(()),
            Expression::Group(inner) | Expression::Try(inner) => self.expression(inner, depth),
            Expression::List { items, .. } => {
                for item in items {
                    self.expression(item, depth)?;
                }
                Ok(())
            }
            Expression::Unary { operand, .. } => self.expression(operand, depth),
            Expression::Borrow { target, .. } => self.expression(target, depth),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.expression(base, depth)?;
                self.expression(start, depth)?;
                if let Some(end) = limit {
                    self.expression(end, depth)?;
                }
                Ok(())
            }
            Expression::Binary { left, right, .. } => {
                self.expression(left, depth)?;
                self.expression(right, depth)
            }
            Expression::Member { base, .. } => self.expression(base, depth),
            Expression::Call {
                callee, arguments, ..
            } => {
                self.expression(callee, depth)?;
                for arg in arguments {
                    self.expression(arg, depth)?;
                }
                Ok(())
            }
            Expression::Meta(block) => self.block(block, depth + 1),
            Expression::Record { fields, .. } => {
                for field in fields {
                    self.expression(&field.value, depth)?;
                }
                Ok(())
            }
            Expression::Switch(dispatch) => self.dispatch(dispatch, depth),
            Expression::Bucket(partition) => self.partition(partition, depth),
            Expression::Loop(kind) => self.repetition(kind, depth),
            Expression::Function { body, .. } => self.body(body, depth),
        }
    }

    fn body(&self, body: &Body, depth: usize) -> Result<(), Finding> {
        match body {
            Body::Block(block) => self.block(block, depth + 1),
            Body::Switch(dispatch) => self.dispatch(dispatch, depth),
            Body::Bucket(partition) => self.partition(partition, depth),
            Body::Loop(kind) => self.repetition(kind, depth),
        }
    }
}
