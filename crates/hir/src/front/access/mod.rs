mod expr;

pub use crate::front::summary::{Exports, summarize};

use crate::front::summary::peeled;
use simplex::Finding;
use simplex::tree::{Decorator, Item, Parameter, Program, Spec, Statement};
use std::collections::BTreeMap;

pub fn verify(modules: &BTreeMap<String, Exports>, program: &Program) -> Result<(), Finding> {
    Access::new(modules).items(&program.items)
}

pub(super) struct Access<'a> {
    pub(super) modules: &'a BTreeMap<String, Exports>,
    pub(super) locals: BTreeMap<String, (String, String)>,
}

impl<'a> Access<'a> {
    pub(super) fn new(modules: &'a BTreeMap<String, Exports>) -> Self {
        Self {
            modules,
            locals: BTreeMap::new(),
        }
    }

    pub(super) fn child(&self) -> Self {
        Self {
            modules: self.modules,
            locals: self.locals.clone(),
        }
    }

    pub(super) fn items(&mut self, list: &[Item]) -> Result<(), Finding> {
        list.iter().try_for_each(|item| self.statement(item))
    }

    fn decorators(&self, list: &[Decorator]) -> Result<(), Finding> {
        for dec in list {
            if let Some(prefix) = &dec.module
                && let Some(exports) = self.modules.get(prefix)
                && let Some(code) = exports.value(&dec.name)
            {
                return Err(Finding {
                    code,
                    line: dec.line,
                    column: dec.column,
                });
            }
        }
        Ok(())
    }

    fn statement(&mut self, item: &Item) -> Result<(), Finding> {
        let pos = (item.line, item.column);
        match &item.statement {
            Statement::Struct(node) => {
                self.decorators(&node.decorators)?;
                node.fields.iter().try_for_each(|f| self.spec(&f.spec, pos))
            }
            Statement::Enum(node) => {
                self.decorators(&node.decorators)?;
                node.variants
                    .iter()
                    .flat_map(|v| &v.payloads)
                    .try_for_each(|s| self.spec(s, pos))
            }
            Statement::Interface(node) => node.methods.iter().try_for_each(|m| {
                self.parameters(&m.inputs, pos)?;
                m.output.as_ref().map_or(Ok(()), |s| self.spec(s, pos))
            }),
            Statement::Alias(node) => self.spec(&node.target, pos),
            Statement::Extern(node) => {
                self.parameters(&node.inputs, pos)?;
                node.output.as_ref().map_or(Ok(()), |s| self.spec(s, pos))
            }
            Statement::Bind(node) => {
                self.decorators(&node.decorators)?;
                if let Some(spec) = &node.annotation {
                    self.spec(spec, pos)?;
                    self.remember(&node.name, spec);
                } else if let Some(pair) = self.origin(&node.value) {
                    self.locals.insert(node.name.clone(), pair);
                }
                self.expression(&node.value)
            }
            Statement::Conform(node) => {
                self.spec(&node.contract, pos)?;
                node.methods
                    .iter()
                    .try_for_each(|m| self.expression(&m.value))
            }
            Statement::Assign { target, value, .. } => {
                self.expression(target)?;
                self.expression(value)
            }
            Statement::For { source, body, .. } => {
                self.expression(source)?;
                self.child().items(&body.items)
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.expression(expr),
            _ => Ok(()),
        }
    }

    pub(super) fn parameters(
        &self,
        list: &[Parameter],
        fallback: (usize, usize),
    ) -> Result<(), Finding> {
        for param in list {
            let pos = match param.line > 0 {
                true => (param.line, param.column),
                false => fallback,
            };
            if let Some(spec) = &param.annotation {
                self.spec(spec, pos)?;
            }
        }
        Ok(())
    }

    pub(super) fn remember(&mut self, name: &str, spec: &Spec) {
        if let Some((Some(prefix), kind)) = peeled(spec) {
            self.locals.insert(name.to_owned(), (prefix, kind));
        }
    }

    pub(super) fn spec(&self, spec: &Spec, pos: (usize, usize)) -> Result<(), Finding> {
        match spec {
            Spec::Named {
                module,
                name,
                arguments,
            } => {
                if let Some(prefix) = module
                    && let Some(exports) = self.modules.get(prefix)
                    && let Some(code) = exports.kind(name)
                {
                    return Err(Finding {
                        code,
                        line: pos.0,
                        column: pos.1,
                    });
                }
                arguments.iter().try_for_each(|arg| self.spec(arg, pos))
            }
            Spec::Borrow { target, .. } | Spec::Slice(target) => self.spec(target, pos),
            Spec::Callable { inputs, output, .. } => {
                inputs.iter().try_for_each(|i| self.spec(i, pos))?;
                output.as_deref().map_or(Ok(()), |out| self.spec(out, pos))
            }
        }
    }
}
