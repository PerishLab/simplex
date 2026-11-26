mod access;
mod graph;

use crate::code::{Code, Finding};
use crate::tree::{
    Block, Body, Branch, Dispatch, Expression, Item, Loop, Outcome, Parameter, Partition, Program,
    Statement,
};
use std::path::Path;

const ARITY: usize = 4;
const CHAIN: usize = 3;

pub fn check(file: Option<&str>, dir: Option<&Path>, program: &Program) -> Result<(), Finding> {
    if let Some(name) = file
        && matches!(name, "mod.sx" | "index.sx" | "lib.sx")
    {
        return Err(Finding {
            code: Code::Barrel,
            line: 1,
            column: 1,
        });
    }
    Topology.items(&program.items)?;
    if let (Some(name), Some(folder)) = (file, dir) {
        graph::verify(name, folder, program)?;
    }
    Ok(())
}

struct Topology;

impl Topology {
    fn items(&self, list: &[Item]) -> Result<(), Finding> {
        for item in list {
            self.statement(item)?;
        }
        Ok(())
    }

    fn statement(&self, item: &Item) -> Result<(), Finding> {
        match &item.statement {
            Statement::Extern(ext) => arity(&ext.inputs, (item.line, item.column)),
            Statement::Interface(contract) => {
                for req in &contract.methods {
                    arity(&req.inputs, (item.line, item.column))?;
                }
                Ok(())
            }
            Statement::Bind(binding) => self.expression(&binding.value),
            Statement::Conform(conf) => {
                for entry in &conf.methods {
                    self.expression(&entry.value)?;
                }
                Ok(())
            }
            Statement::Assign { target, value, .. } => {
                self.expression(target)?;
                self.expression(value)
            }
            Statement::For { source, body, .. } => {
                self.expression(source)?;
                self.block(body)
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.expression(expr),
            _ => Ok(()),
        }
    }

    fn block(&self, block: &Block) -> Result<(), Finding> {
        self.items(&block.items)
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
        match &arm.outcome {
            Outcome::Value(expr) | Outcome::Break(Some(expr)) => self.expression(expr),
            Outcome::Break(None) | Outcome::Continue => Ok(()),
            Outcome::Block(block) => self.block(block),
        }
    }

    fn expression(&self, expr: &Expression) -> Result<(), Finding> {
        match expr {
            Expression::Group(inner) | Expression::Try(inner) => self.expression(inner),
            Expression::Unary { operand, .. } => self.expression(operand),
            Expression::Borrow { target, .. } => self.expression(target),
            Expression::Index {
                base, start, limit, ..
            } => {
                self.expression(base)?;
                self.expression(start)?;
                if let Some(end) = limit {
                    self.expression(end)?;
                }
                Ok(())
            }
            Expression::Binary { left, right, .. } => {
                self.expression(left)?;
                self.expression(right)
            }
            Expression::Member {
                base, line, column, ..
            } => {
                if segments(expr) > CHAIN {
                    return Err(Finding {
                        code: Code::Path,
                        line: *line,
                        column: *column,
                    });
                }
                self.expression(base)
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                self.expression(callee)?;
                for arg in arguments {
                    self.expression(arg)?;
                }
                Ok(())
            }
            Expression::Record { fields, .. } => {
                for field in fields {
                    self.expression(&field.value)?;
                }
                Ok(())
            }
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            Expression::Function { inputs, body, .. } => {
                arity(inputs, (1, 1))?;
                self.body(body)
            }
            _ => Ok(()),
        }
    }

    fn body(&self, body: &Body) -> Result<(), Finding> {
        match body {
            Body::Block(block) => self.block(block),
            Body::Switch(dispatch) => self.dispatch(dispatch),
            Body::Bucket(partition) => self.partition(partition),
            Body::Loop(kind) => self.repetition(kind),
        }
    }
}

fn arity(inputs: &[Parameter], fallback: (usize, usize)) -> Result<(), Finding> {
    if let Some(fifth) = inputs.get(ARITY) {
        return Err(Finding {
            code: Code::Arity,
            line: if fifth.line > 0 {
                fifth.line
            } else {
                fallback.0
            },
            column: if fifth.column > 0 {
                fifth.column
            } else {
                fallback.1
            },
        });
    }
    Ok(())
}

fn segments(expr: &Expression) -> usize {
    match expr {
        Expression::Member { base, .. } => 1 + segments(base),
        _ => 1,
    }
}
