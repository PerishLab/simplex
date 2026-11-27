use simplex::Code;
use simplex::tree::{Expression, Program, Spec, Statement, Structure};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct Exports {
    pub linked: bool,
    pub types: BTreeMap<String, bool>,
    pub values: BTreeMap<String, bool>,
    pub shared: BTreeSet<String>,
    pub structs: BTreeMap<String, Structure>,
    pub returns: BTreeMap<String, String>,
}

impl Exports {
    pub fn value(&self, field: &str) -> Option<Code> {
        if self.linked {
            return (!self.shared.contains(field)).then_some(Code::Boundary);
        }
        let visible = self.values.get(field).copied().unwrap_or(false)
            || self.types.get(field).copied().unwrap_or(false);
        (!visible).then_some(Code::Visibility)
    }

    pub fn kind(&self, name: &str) -> Option<Code> {
        if self.linked {
            return (!self.shared.contains(name)).then_some(Code::Boundary);
        }
        (!self.types.get(name).copied().unwrap_or(false)).then_some(Code::Visibility)
    }

    pub fn constructible(&self, name: &str) -> Option<Code> {
        if self.linked && !self.shared.contains(name) {
            return Some(Code::Boundary);
        }
        let open = self
            .structs
            .get(name)
            .is_some_and(|item| item.public && item.fields.iter().all(|slot| slot.public));
        (!open).then_some(Code::Visibility)
    }

    pub fn readable(&self, kind: &str, field: &str) -> bool {
        let Some(structure) = self.structs.get(kind) else {
            return true;
        };
        structure
            .fields
            .iter()
            .find(|slot| slot.name == field)
            .is_none_or(|slot| slot.public)
    }

    fn expose(&mut self, name: &str, visible: bool, shared: bool) {
        if visible && shared {
            self.shared.insert(name.to_owned());
        }
    }

    fn record(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Struct(item) => {
                self.types.insert(item.name.clone(), item.public);
                self.structs.insert(item.name.clone(), item.clone());
                self.expose(&item.name, item.public, item.dynamic);
            }
            Statement::Enum(item) => {
                self.types.insert(item.name.clone(), item.public);
                self.expose(&item.name, item.public, item.dynamic);
            }
            Statement::Interface(item) => {
                self.types.insert(item.name.clone(), item.public);
                self.expose(&item.name, item.public, item.dynamic);
            }
            Statement::Alias(item) => {
                self.types.insert(item.name.clone(), item.public);
                self.expose(&item.name, item.public, item.dynamic);
            }
            Statement::Bind(item) => {
                self.values.insert(item.name.clone(), item.public);
                self.expose(&item.name, item.public, item.dynamic);
                if item.public
                    && let Expression::Function {
                        output: Some(spec), ..
                    } = &item.value
                    && let Some((None, target)) = peeled(spec)
                {
                    self.returns.insert(item.name.clone(), target);
                }
            }
            Statement::Extern(item) => {
                self.values.insert(item.name.clone(), item.public);
                self.expose(&item.name, item.public, item.dynamic);
            }
            _ => {}
        }
    }
}

pub fn summarize(program: &Program, linked: bool) -> Exports {
    let mut out = Exports {
        linked,
        ..Exports::default()
    };
    for item in &program.items {
        out.record(&item.statement);
    }
    out
}

pub fn peeled(spec: &Spec) -> Option<(Option<String>, String)> {
    match spec {
        Spec::Named { module, name, .. } => Some((module.clone(), name.clone())),
        Spec::Borrow { target, .. } => peeled(target),
        _ => None,
    }
}
