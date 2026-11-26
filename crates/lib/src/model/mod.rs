mod borrow;
mod call;
mod check;
mod count;
mod fiber;
mod flow;
mod infer;
mod meta;
mod usage;

use crate::code::{Code, Finding};
use crate::tree::{
    Body, Conformance, Contract, Enumeration, Expression, Generic, Item, Parameter, Program, Spec,
    Statement, Structure,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn check(program: &Program) -> Result<(), Finding> {
    let mut table = Table::build(program)?;
    table.decorators(program)?;
    usage::verify(&table, program)?;
    table.conformances()?;
    check::verify(&table, program)?;
    table.evaluate(program)?;
    table.fibers(program)?;
    table.verify(program)
}

pub(crate) struct Signature {
    pub(crate) asynchronous: bool,
    pub(crate) generics: Vec<Generic>,
    pub(crate) inputs: Vec<Option<Spec>>,
    pub(crate) output: Option<Spec>,
}

pub(crate) struct Claim {
    pub(crate) subject: String,
    pub(crate) contract: String,
    pub(crate) methods: Vec<String>,
    pub(crate) line: usize,
    pub(crate) column: usize,
}

#[derive(Default)]
pub(crate) struct Table {
    pub(crate) structs: BTreeMap<String, Structure>,
    pub(crate) enums: BTreeMap<String, Enumeration>,
    pub(crate) interfaces: BTreeMap<String, Contract>,
    pub(crate) aliases: BTreeMap<String, Spec>,
    pub(crate) functions: BTreeMap<String, Signature>,
    pub(crate) bodies: BTreeMap<String, (Vec<Parameter>, Body)>,
    pub(crate) externals: BTreeSet<String>,
    pub(crate) methods: BTreeMap<(String, String), Signature>,
    pub(crate) claims: Vec<Claim>,
    pub(crate) satisfied: BTreeSet<(String, String)>,
    pub(crate) copies: BTreeSet<String>,
    pub(crate) callers: BTreeSet<String>,
    pub(crate) assets: BTreeSet<String>,
}

impl Table {
    fn build(program: &Program) -> Result<Self, Finding> {
        let mut table = Self::default();
        for item in &program.items {
            table.register(item);
        }
        Ok(table)
    }

    fn register(&mut self, item: &Item) {
        match &item.statement {
            Statement::Struct(entry) => {
                self.structs.insert(entry.name.clone(), entry.clone());
            }
            Statement::Enum(entry) => {
                self.enums.insert(entry.name.clone(), entry.clone());
            }
            Statement::Interface(entry) => {
                self.interfaces.insert(entry.name.clone(), entry.clone());
            }
            Statement::Alias(entry) => {
                self.aliases
                    .insert(entry.name.clone(), entry.target.clone());
            }
            Statement::Conform(entry) => self.bind(entry, (item.line, item.column)),
            Statement::Bind(binding) => {
                if let Some(sig) = callable(&binding.value) {
                    self.functions.insert(binding.name.clone(), sig);
                }
                if let Expression::Function { inputs, body, .. } = &binding.value {
                    self.bodies
                        .insert(binding.name.clone(), (inputs.clone(), body.clone()));
                }
            }
            Statement::Extern(ext) => {
                self.externals.insert(ext.name.clone());
                self.functions.insert(
                    ext.name.clone(),
                    Signature {
                        asynchronous: false,
                        generics: Vec::new(),
                        inputs: ext.inputs.iter().map(|p| p.annotation.clone()).collect(),
                        output: ext.output.clone(),
                    },
                );
            }
            _ => {}
        }
    }

    fn bind(&mut self, entry: &Conformance, pos: (usize, usize)) {
        let Spec::Named { module, name, .. } = &entry.contract else {
            return;
        };
        let mut names = Vec::new();
        for method in &entry.methods {
            names.push(method.name.clone());
            if let Some(sig) = callable(&method.value) {
                self.methods
                    .insert((entry.subject.clone(), method.name.clone()), sig);
            }
        }
        if module.is_some() {
            return;
        }
        if name == "Copy" {
            self.copies.insert(entry.subject.clone());
        }
        self.claims.push(Claim {
            subject: entry.subject.clone(),
            contract: name.clone(),
            methods: names,
            line: pos.0,
            column: pos.1,
        });
        self.satisfied.insert((entry.subject.clone(), name.clone()));
    }

    fn conformances(&self) -> Result<(), Finding> {
        for claim in &self.claims {
            let target = self.resolve(&claim.contract);
            if target == "Copy" {
                if !claim.methods.is_empty() {
                    return Err(Finding {
                        code: Code::Conformance,
                        line: claim.line,
                        column: claim.column,
                    });
                }
                continue;
            }
            if self.structs.contains_key(&target) || self.enums.contains_key(&target) {
                return Err(Finding {
                    code: Code::Inheritance,
                    line: claim.line,
                    column: claim.column,
                });
            }
            if let Some(contract) = self.interfaces.get(&target) {
                self.satisfy(claim, contract)?;
            }
        }
        Ok(())
    }

    fn satisfy(&self, claim: &Claim, contract: &Contract) -> Result<(), Finding> {
        if claim.methods.len() != contract.methods.len() {
            return Err(Finding {
                code: Code::Conformance,
                line: claim.line,
                column: claim.column,
            });
        }
        for req in &contract.methods {
            let key = (claim.subject.clone(), req.name.clone());
            let Some(found) = self.methods.get(&key) else {
                return Err(Finding {
                    code: Code::Conformance,
                    line: claim.line,
                    column: claim.column,
                });
            };
            if found.asynchronous != req.asynchronous {
                return Err(Finding {
                    code: Code::Mismatch,
                    line: claim.line,
                    column: claim.column,
                });
            }
            if found.inputs.len() != req.inputs.len()
                || found.output.is_some() != req.output.is_some()
            {
                return Err(Finding {
                    code: Code::Conformance,
                    line: claim.line,
                    column: claim.column,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn resolve(&self, name: &str) -> String {
        let mut current = name.to_owned();
        for _ in 0..8 {
            let Some(Spec::Named { name: next, .. }) = self.aliases.get(&current) else {
                break;
            };
            current.clone_from(next);
        }
        current
    }

    pub(crate) fn field(&self, owner: &str, field: &str) -> Option<Spec> {
        let resolved = self.resolve(owner);
        let entry = self.structs.get(&resolved)?;
        entry
            .fields
            .iter()
            .find(|slot| slot.name == field)
            .map(|slot| slot.spec.clone())
    }

    pub(crate) fn method(&self, owner: &str, name: &str) -> Option<&Signature> {
        let resolved = self.resolve(owner);
        self.methods.get(&(resolved, name.to_owned()))
    }

    pub(crate) fn nominal(&self, spec: &Spec) -> Option<String> {
        match spec {
            Spec::Named {
                module: None, name, ..
            } => Some(self.resolve(name)),
            Spec::Borrow { target, .. } => self.nominal(target),
            _ => None,
        }
    }
}

fn callable(expr: &Expression) -> Option<Signature> {
    let Expression::Function {
        asynchronous,
        generics,
        inputs,
        output,
        ..
    } = expr
    else {
        return None;
    };
    Some(Signature {
        asynchronous: *asynchronous,
        generics: generics.clone(),
        inputs: inputs
            .iter()
            .map(|param| param.annotation.clone())
            .collect(),
        output: output.clone(),
    })
}
