use super::loader::{Graph, Unit};
use super::types::{Kind, primitive};
use simplex::tree::{Alias, Contract, Enumeration, Generic, Spec, Statement, Structure};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) type Subst = BTreeMap<String, Kind>;

pub(crate) struct Catalog<'a> {
    pub(crate) units: BTreeMap<String, &'a Unit>,
    pub(crate) structs: BTreeMap<(String, String), &'a Structure>,
    pub(crate) enums: BTreeMap<(String, String), &'a Enumeration>,
    pub(crate) interfaces: BTreeMap<(String, String), &'a Contract>,
    pub(crate) aliases: BTreeMap<(String, String), &'a Alias>,
    pub(crate) active: BTreeSet<String>,
}

impl<'a> Catalog<'a> {
    pub(crate) fn new(graph: &'a Graph) -> Self {
        let mut catalog = Self {
            units: BTreeMap::new(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
            interfaces: BTreeMap::new(),
            aliases: BTreeMap::new(),
            active: BTreeSet::new(),
        };
        for unit in &graph.units {
            catalog.ingest(unit);
        }
        catalog
    }

    fn ingest(&mut self, unit: &'a Unit) {
        self.units.insert(unit.id.clone(), unit);
        for item in &unit.program.items {
            self.register(&unit.id, &item.statement);
        }
    }

    fn register(&mut self, owner: &str, stmt: &'a Statement) {
        let key = |name: &str| (owner.to_owned(), name.to_owned());
        match stmt {
            Statement::Struct(def) => {
                self.structs.insert(key(&def.name), def);
            }
            Statement::Enum(def) => {
                self.enums.insert(key(&def.name), def);
            }
            Statement::Interface(def) => {
                self.interfaces.insert(key(&def.name), def);
            }
            Statement::Alias(def) => {
                self.aliases.insert(key(&def.name), def);
            }
            _ => {}
        }
    }

    pub(crate) fn resolve(
        &mut self,
        owner: &str,
        spec: &Spec,
        subst: &Subst,
        depth: usize,
    ) -> Kind {
        if depth > 16 {
            return Kind::Void;
        }
        match spec {
            Spec::Slice(inner) => {
                Kind::Slice(Box::new(self.resolve(owner, inner, subst, depth + 1)))
            }
            Spec::Borrow { target, .. } => self.borrowed(owner, target, subst, depth + 1),
            Spec::Callable {
                asynchronous,
                inputs,
                output,
            } => self.callable(
                owner,
                (*asynchronous, inputs, output.as_deref()),
                subst,
                depth + 1,
            ),
            Spec::Named {
                module,
                name,
                arguments,
            } => self.named(
                (owner, module.as_deref()),
                (name, arguments),
                subst,
                depth + 1,
            ),
        }
    }

    fn callable(
        &mut self,
        owner: &str,
        sig: (bool, &[Spec], Option<&Spec>),
        subst: &Subst,
        depth: usize,
    ) -> Kind {
        let (asynchronous, inputs, output) = sig;
        let params = inputs
            .iter()
            .map(|item| self.resolve(owner, item, subst, depth))
            .collect();
        let raw = output.map_or(Kind::Void, |item| self.resolve(owner, item, subst, depth));
        let ret = if asynchronous {
            Kind::Fiber(Box::new(raw))
        } else {
            raw
        };
        Kind::Function(params, Box::new(ret))
    }

    fn borrowed(&mut self, owner: &str, target: &Spec, subst: &Subst, depth: usize) -> Kind {
        match target {
            Spec::Slice(elem) => Kind::Slice(Box::new(self.resolve(owner, elem, subst, depth))),
            other => {
                let inner = self.resolve(owner, other, subst, depth);
                if matches!(inner, Kind::Trait(_)) {
                    return inner;
                }
                Kind::Pointer(Box::new(inner))
            }
        }
    }

    fn named(
        &mut self,
        scope: (&str, Option<&str>),
        pair: (&str, &[Spec]),
        subst: &Subst,
        depth: usize,
    ) -> Kind {
        let (owner, module) = scope;
        let (name, arguments) = pair;
        if module.is_none()
            && let Some(found) = self.intrinsic(owner, name, arguments, (subst, depth))
        {
            return found;
        }
        let args: Vec<Kind> = arguments
            .iter()
            .map(|item| self.resolve(owner, item, subst, depth))
            .collect();
        let Some(target) = self.locate(owner, module) else {
            let ext = self.unresolved(owner, module.unwrap_or_default(), name);
            return Kind::External(ext, args);
        };
        let key = (target.clone(), name.to_owned());
        if let Some(alias) = self.aliases.get(&key).copied() {
            let next = bind(&alias.generics, &args);
            return self.resolve(&target, &alias.target, &next, depth);
        }
        let qualified = qualify(&target, name);
        if self.structs.contains_key(&key) {
            return Kind::Record(qualified, args);
        }
        if self.enums.contains_key(&key) {
            return Kind::Choice(qualified, args);
        }
        if self.interfaces.contains_key(&key) {
            return Kind::Trait(qualified);
        }
        Kind::External(qualified, args)
    }

    fn intrinsic(
        &mut self,
        owner: &str,
        name: &str,
        arguments: &[Spec],
        ctx: (&Subst, usize),
    ) -> Option<Kind> {
        let (subst, depth) = ctx;
        if let Some(bound) = subst.get(name) {
            return Some(bound.clone());
        }
        if let Some(prim) = primitive(name) {
            return Some(prim);
        }
        if name == "Origin" {
            return Some(Kind::Origin);
        }
        let [single] = arguments else {
            return None;
        };
        match name {
            "Slice" => Some(Kind::Slice(Box::new(
                self.resolve(owner, single, subst, depth),
            ))),
            "List" => Some(Kind::List(Box::new(
                self.resolve(owner, single, subst, depth),
            ))),
            "Fiber" => Some(Kind::Fiber(Box::new(
                self.resolve(owner, single, subst, depth),
            ))),
            _ => None,
        }
    }

    fn locate(&self, owner: &str, module: Option<&str>) -> Option<String> {
        let Some(alias) = module else {
            return Some(owner.to_owned());
        };
        self.units.get(owner)?.locals.get(alias).cloned()
    }

    fn unresolved(&self, owner: &str, alias: &str, name: &str) -> String {
        if let Some(unit) = self.units.get(owner)
            && let Some(dep) = unit.external.get(alias)
        {
            return format!("ext:{dep}.{name}");
        }
        format!("{alias}.{name}")
    }
}

pub(crate) fn placeholders(list: &[Generic]) -> Subst {
    list.iter()
        .map(|item| (item.name.clone(), Kind::Param(item.name.clone())))
        .collect()
}

pub(crate) fn bind(list: &[Generic], args: &[Kind]) -> Subst {
    list.iter()
        .zip(args)
        .map(|(item, arg)| (item.name.clone(), arg.clone()))
        .collect()
}

fn qualify(owner: &str, name: &str) -> String {
    match owner.is_empty() {
        true => name.to_owned(),
        false => format!("{owner}.{name}"),
    }
}
