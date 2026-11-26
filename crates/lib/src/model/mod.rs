mod check;
mod count;
mod infer;
mod usage;

use crate::code::{Code, Finding};
use crate::tree::{
    Contract, Enumeration, Expression, Generic, Item, Program, Spec, Statement, Structure,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn check(program: &Program) -> Result<(), Finding> {
    usage::verify(program)?;
    let table = Table::build(program)?;
    table.conformances()?;
    check::verify(&table, program)
}

pub(crate) struct Signature {
    pub(crate) generics: Vec<Generic>,
    pub(crate) inputs: Vec<Option<Spec>>,
    pub(crate) output: Option<Spec>,
}

pub(crate) struct Claim {
    pub(crate) subject: String,
    pub(crate) contract: String,
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
    pub(crate) methods: BTreeMap<(String, String), Signature>,
    pub(crate) claims: Vec<Claim>,
    pub(crate) satisfied: BTreeSet<(String, String)>,
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
            Statement::Conform {
                subject,
                contract: Spec::Named { name, .. },
            } => {
                self.claims.push(Claim {
                    subject: subject.clone(),
                    contract: name.clone(),
                    line: item.line,
                    column: item.column,
                });
                self.satisfied.insert((subject.clone(), name.clone()));
            }
            Statement::Method(entry) => {
                if let Some(sig) = callable(&entry.value) {
                    self.methods
                        .insert((entry.receiver.clone(), entry.name.clone()), sig);
                }
            }
            Statement::Bind(binding) => {
                if let Some(sig) = callable(&binding.value) {
                    self.functions.insert(binding.name.clone(), sig);
                }
            }
            _ => {}
        }
    }

    fn conformances(&self) -> Result<(), Finding> {
        for claim in &self.claims {
            let target = self.resolve(&claim.contract);
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
        for req in &contract.methods {
            let key = (claim.subject.clone(), req.name.clone());
            let Some(found) = self.methods.get(&key) else {
                return Err(Finding {
                    code: Code::Conformance,
                    line: claim.line,
                    column: claim.column,
                });
            };
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
}

fn callable(expr: &Expression) -> Option<Signature> {
    let Expression::Function {
        generics,
        inputs,
        output,
        ..
    } = expr
    else {
        return None;
    };
    Some(Signature {
        generics: generics.clone(),
        inputs: inputs
            .iter()
            .map(|param| param.annotation.clone())
            .collect(),
        output: output.clone(),
    })
}
