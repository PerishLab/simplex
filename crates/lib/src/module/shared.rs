use crate::code::{Code, Finding};
use crate::tree::{
    Alias, Binding, Contract, Enumeration, Expression, Item, Program, Requirement, Spec, Statement,
    Structure,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Boundary {
    values: BTreeSet<String>,
    contracts: BTreeSet<String>,
    structs: BTreeMap<String, Structure>,
}

pub(crate) fn verify(program: &Program) -> Result<(), Finding> {
    let mut boundary = Boundary::default();
    for item in &program.items {
        boundary.collect(&item.statement);
    }
    for item in &program.items {
        boundary.item(item)?;
    }
    Ok(())
}

impl Boundary {
    fn collect(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Struct(s) => {
                self.structs.insert(s.name.clone(), s.clone());
                if s.dynamic {
                    self.values.insert(s.name.clone());
                }
            }
            Statement::Enum(e) if e.dynamic => {
                self.values.insert(e.name.clone());
            }
            Statement::Alias(a) if a.dynamic => {
                self.values.insert(a.name.clone());
            }
            Statement::Interface(c) if c.dynamic => {
                self.contracts.insert(c.name.clone());
            }
            _ => {}
        }
    }

    fn item(&self, item: &Item) -> Result<(), Finding> {
        let pos = (item.line, item.column);
        match &item.statement {
            Statement::Struct(s) if s.dynamic => self.structure(s, pos),
            Statement::Enum(e) if e.dynamic => self.enumeration(e, pos),
            Statement::Interface(c) if c.dynamic => self.contract(c, pos),
            Statement::Alias(a) if a.dynamic => self.alias(a, pos),
            Statement::Bind(b) if b.dynamic => self.binding(b, pos),
            _ => Ok(()),
        }
    }

    fn structure(&self, item: &Structure, pos: (usize, usize)) -> Result<(), Finding> {
        if !item.generics.is_empty() {
            return Err(fault(Code::Generic, pos));
        }
        if item
            .fields
            .iter()
            .any(|slot| !slot.public || !self.stable(&slot.spec, false))
        {
            return Err(fault(Code::Layout, pos));
        }
        Ok(())
    }

    fn enumeration(&self, item: &Enumeration, pos: (usize, usize)) -> Result<(), Finding> {
        if !item.generics.is_empty() {
            return Err(fault(Code::Generic, pos));
        }
        if item
            .variants
            .iter()
            .flat_map(|v| &v.payloads)
            .any(|spec| !self.stable(spec, false))
        {
            return Err(fault(Code::Layout, pos));
        }
        Ok(())
    }

    fn contract(&self, item: &Contract, pos: (usize, usize)) -> Result<(), Finding> {
        if !item.generics.is_empty() {
            return Err(fault(Code::Generic, pos));
        }
        for req in &item.methods {
            self.requirement(req, pos)?;
        }
        Ok(())
    }

    fn requirement(&self, req: &Requirement, pos: (usize, usize)) -> Result<(), Finding> {
        if req.inputs.iter().any(|p| p.constant) {
            return Err(fault(Code::Generic, pos));
        }
        if req.asynchronous {
            return Err(fault(Code::Layout, pos));
        }
        let inputs = req
            .inputs
            .iter()
            .all(|p| p.annotation.as_ref().is_some_and(|s| self.stable(s, true)));
        let output = req.output.as_ref().is_none_or(|s| self.stable(s, true));
        if !inputs || !output {
            return Err(fault(Code::Layout, pos));
        }
        Ok(())
    }

    fn alias(&self, item: &Alias, pos: (usize, usize)) -> Result<(), Finding> {
        if !item.generics.is_empty() {
            return Err(fault(Code::Generic, pos));
        }
        if !self.instantiated(&item.target) && !self.stable(&item.target, false) {
            return Err(fault(Code::Layout, pos));
        }
        Ok(())
    }

    fn instantiated(&self, target: &Spec) -> bool {
        let Spec::Named {
            module: None,
            name,
            arguments,
        } = target
        else {
            return false;
        };
        let Some(structure) = self.structs.get(name) else {
            return false;
        };
        if arguments.is_empty() || arguments.len() != structure.generics.len() {
            return false;
        }
        if !arguments.iter().all(|arg| self.stable(arg, false)) {
            return false;
        }
        let map: BTreeMap<&str, &Spec> = structure
            .generics
            .iter()
            .map(|g| g.name.as_str())
            .zip(arguments)
            .collect();
        structure
            .fields
            .iter()
            .all(|slot| slot.public && self.substituted(&slot.spec, &map))
    }

    fn substituted(&self, spec: &Spec, map: &BTreeMap<&str, &Spec>) -> bool {
        if let Spec::Named {
            module: None,
            name,
            arguments,
        } = spec
            && arguments.is_empty()
            && let Some(bound) = map.get(name.as_str())
        {
            return self.stable(bound, false);
        }
        self.stable(spec, false)
    }

    fn binding(&self, item: &Binding, pos: (usize, usize)) -> Result<(), Finding> {
        if item
            .decorators
            .iter()
            .any(|d| d.module.is_none() && d.name == "caller")
        {
            return Err(fault(Code::Generic, pos));
        }
        if let Some(spec) = &item.annotation
            && !self.stable(spec, false)
        {
            return Err(fault(Code::Layout, pos));
        }
        let Expression::Function {
            asynchronous,
            generics,
            inputs,
            output,
            ..
        } = &item.value
        else {
            return Ok(());
        };
        if !generics.is_empty() || inputs.iter().any(|p| p.constant) {
            return Err(fault(Code::Generic, pos));
        }
        if *asynchronous {
            return Err(fault(Code::Layout, pos));
        }
        let params = inputs
            .iter()
            .all(|p| p.annotation.as_ref().is_some_and(|s| self.stable(s, false)));
        let ret = output.as_ref().is_none_or(|s| self.stable(s, false));
        if !params || !ret {
            return Err(fault(Code::Layout, pos));
        }
        Ok(())
    }

    fn stable(&self, spec: &Spec, receiver: bool) -> bool {
        match spec {
            Spec::Named {
                module: Some(_),
                arguments,
                ..
            } => arguments.is_empty(),
            Spec::Named {
                module: None,
                name,
                arguments,
            } => arguments.is_empty() && self.named(name, receiver),
            Spec::Borrow { target, .. } => match target.as_ref() {
                Spec::Slice(elem) => self.stable(elem, receiver),
                Spec::Named {
                    module: None,
                    name,
                    arguments,
                } if arguments.is_empty() && self.contracts.contains(name) => true,
                other => self.stable(other, receiver),
            },
            Spec::Slice(_) => false,
            Spec::Callable {
                asynchronous,
                inputs,
                output,
            } => {
                !*asynchronous
                    && inputs.iter().all(|i| self.stable(i, receiver))
                    && output.as_deref().is_none_or(|o| self.stable(o, receiver))
            }
        }
    }

    fn named(&self, name: &str, receiver: bool) -> bool {
        if fixed(name) || self.values.contains(name) {
            return true;
        }
        receiver && name == "Self"
    }
}

fn fixed(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
    )
}

fn fault(code: Code, pos: (usize, usize)) -> Finding {
    Finding {
        code,
        line: pos.0,
        column: pos.1,
    }
}
