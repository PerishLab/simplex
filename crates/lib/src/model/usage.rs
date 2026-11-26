use super::Table;
use super::count::Counter;
use super::fiber::{mentions, spawns};
use crate::code::{Code, Finding};
use crate::tree::{
    Binding, Block, Body, Branch, Dispatch, Expression, Item, Loop, Outcome, Parameter, Partition,
    Pattern, Program, Statement,
};

pub(crate) fn verify(table: &Table, program: &Program) -> Result<(), Finding> {
    Scanner { table }.items(&program.items, false)
}

struct Scanner<'a> {
    table: &'a Table,
}

impl<'a> Scanner<'a> {
    fn items(&self, list: &[Item], scoped: bool) -> Result<(), Finding> {
        for (index, item) in list.iter().enumerate() {
            self.guard(item, &list[index + 1..], scoped)?;
            self.statement(item)?;
        }
        Ok(())
    }

    fn guard(&self, item: &Item, rest: &[Item], scoped: bool) -> Result<(), Finding> {
        let Statement::Bind(binding) = &item.statement else {
            return Ok(());
        };
        if binding.name == "_" {
            return Err(Finding {
                code: Code::Discard,
                line: item.line,
                column: item.column,
            });
        }
        if !scoped || self.linear(binding) {
            return Ok(());
        }
        let refs = Counter::new(&binding.name).items(rest);
        let needed = if binding.mutable { 1 } else { 2 };
        if refs < needed {
            return Err(Finding {
                code: Code::Usage,
                line: item.line,
                column: item.column,
            });
        }
        Ok(())
    }

    fn linear(&self, binding: &Binding) -> bool {
        binding.annotation.as_ref().is_some_and(mentions) || spawns(self.table, &binding.value)
    }

    fn statement(&self, item: &Item) -> Result<(), Finding> {
        match &item.statement {
            Statement::Bind(binding) => self.expression(&binding.value),
            Statement::Conform(conf) => {
                for entry in &conf.methods {
                    self.expression(&entry.value)?;
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
                self.expression(target)?;
                self.expression(value)
            }
            Statement::For {
                item: name,
                source,
                body,
                ..
            } => {
                if name == "_" {
                    return Err(Finding {
                        code: Code::Discard,
                        line: item.line,
                        column: item.column,
                    });
                }
                self.expression(source)?;
                self.block(body)
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.expression(expr),
        }
    }

    fn block(&self, block: &Block) -> Result<(), Finding> {
        self.items(&block.items, true)
    }

    fn repetition(&self, kind: &Loop) -> Result<(), Finding> {
        match kind {
            Loop::Block(block) => self.block(block),
            Loop::Switch(dispatch) => self.dispatch(dispatch),
            Loop::Bucket(partition) => self.partition(partition),
        }
    }

    fn dispatch(&self, dispatch: &Dispatch) -> Result<(), Finding> {
        self.expression(&dispatch.subject)?;
        for arm in &dispatch.arms {
            if matches!(arm.pattern, Pattern::Wildcard) {
                return Err(Finding {
                    code: Code::Discard,
                    line: arm.line,
                    column: arm.column,
                });
            }
            self.branch(arm)?;
        }
        Ok(())
    }

    fn partition(&self, partition: &Partition) -> Result<(), Finding> {
        if let Some(subject) = &partition.subject {
            self.expression(subject)?;
        }
        for arm in &partition.arms {
            self.branch(arm)?;
        }
        Ok(())
    }

    fn branch(&self, arm: &Branch) -> Result<(), Finding> {
        match &arm.pattern {
            Pattern::Wildcard => {}
            Pattern::Choices(list) => self.many(list)?,
            Pattern::Range { start, end } => {
                self.expression(start)?;
                self.expression(end)?;
            }
            Pattern::Predicate(expr) => self.expression(expr)?,
        }
        match &arm.outcome {
            Outcome::Value(expr) | Outcome::Break(Some(expr)) => self.expression(expr),
            Outcome::Break(None) | Outcome::Continue => Ok(()),
            Outcome::Block(block) => self.block(block),
        }
    }

    fn many(&self, list: &[Expression]) -> Result<(), Finding> {
        for expr in list {
            self.expression(expr)?;
        }
        Ok(())
    }

    fn expression(&self, expr: &Expression) -> Result<(), Finding> {
        match expr {
            Expression::Word { text, line, column } => {
                if text == "_" {
                    return Err(Finding {
                        code: Code::Discard,
                        line: *line,
                        column: *column,
                    });
                }
                Ok(())
            }
            Expression::Number(_) | Expression::Text(_) | Expression::Byte(_) => Ok(()),
            Expression::Group(inner) | Expression::Try(inner) => self.expression(inner),
            Expression::List { items, .. } => self.many(items),
            Expression::Unary { operand, .. }
            | Expression::Borrow {
                target: operand, ..
            } => self.expression(operand),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.expression(base)?;
                self.expression(start)?;
                limit.as_deref().map_or(Ok(()), |end| self.expression(end))
            }
            Expression::Binary { left, right, .. } => {
                self.expression(left)?;
                self.expression(right)
            }
            Expression::Member { base, .. } => self.expression(base),
            Expression::Call {
                callee, arguments, ..
            } => {
                self.expression(callee)?;
                self.many(arguments)
            }
            Expression::Meta(block) => self.block(block),
            Expression::Record { fields, .. } => {
                for field in fields {
                    self.expression(&field.value)?;
                }
                Ok(())
            }
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            Expression::Function { inputs, body, .. } => self.callable(inputs, body),
        }
    }

    fn callable(&self, inputs: &[Parameter], body: &Body) -> Result<(), Finding> {
        for param in inputs {
            if param.name == "_" {
                return Err(Finding {
                    code: Code::Discard,
                    line: param.line,
                    column: param.column,
                });
            }
            if Counter::new(&param.name).body(body) == 0 {
                return Err(Finding {
                    code: Code::Usage,
                    line: param.line,
                    column: param.column,
                });
            }
        }
        match body {
            Body::Block(block) => self.block(block),
            Body::Switch(dispatch) => self.dispatch(dispatch),
            Body::Bucket(partition) => self.partition(partition),
            Body::Loop(kind) => self.repetition(kind),
        }
    }
}
