use crate::front::loader::{Bundle, Meta, Source};
use crate::front::scan::placeholders;
use crate::layout::Mangle;
use simplex::tree::{
    Alias, Binding, Conformance, Contract, Decorator, Enumeration, Expression, External, Item,
    Spec, Statement, Structure,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Claim {
    pub module: String,
    pub subject: String,
    pub target: (String, String),
    pub methods: BTreeMap<String, Expression>,
    pub derived: bool,
}

#[derive(Default)]
pub struct World {
    pub order: Vec<String>,
    pub files: BTreeMap<String, String>,
    pub meta: BTreeMap<String, Meta>,
    pub imports: BTreeMap<(String, String), (String, bool)>,
    pub structs: BTreeMap<(String, String), Structure>,
    pub enums: BTreeMap<(String, String), Enumeration>,
    pub interfaces: BTreeMap<(String, String), Contract>,
    pub aliases: BTreeMap<(String, String), Alias>,
    pub bindings: BTreeMap<(String, String), Binding>,
    pub externals: BTreeMap<(String, String), External>,
    pub claims: Vec<Claim>,
    pub copies: BTreeSet<(String, String)>,
    pub callers: BTreeSet<(String, String)>,
    pub dynamics: Vec<(String, String)>,
    pub programs: BTreeMap<String, Vec<Item>>,
}

impl World {
    pub fn build(bundle: &Bundle) -> Self {
        let mut world = Self::default();
        for (key, shared) in &bundle.shared {
            world.dynamics.push((key.clone(), shared.library.clone()));
            world.attach(&shared.entry);
            world.ingest(&shared.entry, true, true);
            for sup in &shared.support {
                world.attach(sup);
                world.ingest(sup, true, false);
            }
        }
        for source in &bundle.units {
            world.order.push(source.key.clone());
            world.files.insert(source.key.clone(), source.file.clone());
            world.attach(source);
            world
                .programs
                .insert(source.key.clone(), source.program.items.clone());
            world.ingest(source, false, true);
        }
        for stem in world.order.clone() {
            if let Some(items) = world.programs.get(&stem).cloned() {
                placeholders(&mut world, &stem, &items);
            }
        }
        world
    }

    fn attach(&mut self, source: &Source) {
        self.meta
            .entry(source.key.clone())
            .or_insert_with(|| source.meta.clone());
        for (alias, target) in &source.imports {
            self.imports
                .insert((source.key.clone(), alias.clone()), target.clone());
        }
    }

    fn ingest(&mut self, source: &Source, linked: bool, values: bool) {
        for item in &source.program.items {
            self.register(&source.key, &item.statement, (linked, values));
        }
    }

    fn register(&mut self, stem: &str, stmt: &Statement, mode: (bool, bool)) {
        let (linked, values) = mode;
        let key = |name: &String| (stem.to_owned(), name.clone());
        match stmt {
            Statement::Import(imp) => {
                let pair = (stem.to_owned(), imp.name.clone());
                self.imports.entry(pair).or_insert_with(|| {
                    let leaf = imp.path.last().cloned().unwrap_or_else(|| imp.name.clone());
                    (leaf, imp.dynamic)
                });
            }
            Statement::Struct(s) => {
                self.derives(stem, &s.name, &s.decorators);
                self.structs.insert(key(&s.name), s.clone());
            }
            Statement::Enum(e) => {
                self.derives(stem, &e.name, &e.decorators);
                self.enums.insert(key(&e.name), e.clone());
            }
            Statement::Interface(c) => {
                self.interfaces.insert(key(&c.name), c.clone());
            }
            Statement::Alias(a) => {
                self.aliases.insert(key(&a.name), a.clone());
            }
            Statement::Bind(b) if values => self.binding(stem, b, linked),
            Statement::Extern(ext) if !linked => {
                self.externals.insert(key(&ext.name), ext.clone());
            }
            Statement::Conform(conf) if !linked => self.conform(stem, conf),
            _ => {}
        }
    }

    fn binding(&mut self, stem: &str, b: &Binding, linked: bool) {
        let key = (stem.to_owned(), b.name.clone());
        if b.decorators
            .iter()
            .any(|d| d.module.is_none() && d.name == "caller")
        {
            self.callers.insert(key.clone());
        }
        if !linked || b.dynamic {
            self.bindings.insert(key, b.clone());
        }
    }

    fn derives(&mut self, stem: &str, subject: &str, list: &[Decorator]) {
        for dec in list {
            if dec.module.is_some() || dec.name != "derive" {
                continue;
            }
            for arg in &dec.arguments {
                self.derived(stem, subject, arg);
            }
        }
    }

    fn derived(&mut self, stem: &str, subject: &str, arg: &Expression) {
        let Expression::Word { text, .. } = arg else {
            return;
        };
        if text == "Copy" {
            self.copies.insert((stem.to_owned(), subject.to_owned()));
            return;
        }
        self.claims.push(Claim {
            module: stem.to_owned(),
            subject: subject.to_owned(),
            target: (stem.to_owned(), text.clone()),
            methods: BTreeMap::new(),
            derived: true,
        });
    }

    fn conform(&mut self, stem: &str, conf: &Conformance) {
        let Spec::Named { module, name, .. } = &conf.contract else {
            return;
        };
        let owner = match module {
            Some(alias) => self.target(stem, alias),
            None => stem.to_owned(),
        };
        if module.is_none() && name == "Copy" {
            self.copies.insert((stem.to_owned(), conf.subject.clone()));
            return;
        }
        let methods = conf
            .methods
            .iter()
            .map(|m| (m.name.clone(), m.value.clone()))
            .collect();
        self.claims.push(Claim {
            module: stem.to_owned(),
            subject: conf.subject.clone(),
            target: (owner, name.clone()),
            methods,
            derived: false,
        });
    }

    pub fn coords<'a>(&'a self, fallback: &'a str, key: &'a str) -> (&'a str, &'a str) {
        match self.meta.get(key) {
            Some(meta) => (&meta.ns, &meta.module),
            None => (fallback, key),
        }
    }

    pub fn symbol(&self, fallback: &str, key: &str, name: &str) -> String {
        let (ns, module) = self.coords(fallback, key);
        Mangle::symbol(ns, module, name)
    }

    pub fn table(&self, fallback: &str, key: &str, subject: &str, contract: &str) -> String {
        let (ns, module) = self.coords(fallback, key);
        Mangle::table(ns, module, subject, contract)
    }

    pub fn folder<'a>(&'a self, fallback: &'a Path, key: &str) -> (&'a Path, bool) {
        match self.meta.get(key) {
            Some(meta) => (&meta.root, meta.strict),
            None => (fallback, false),
        }
    }

    pub fn local(&self, key: &str) -> bool {
        self.meta.get(key).is_none_or(|meta| meta.local)
    }

    pub fn prefix(&self, key: &str) -> String {
        match self.meta.get(key) {
            Some(meta) if meta.qualified => format!("{}${}", meta.ns, meta.module),
            Some(meta) => meta.module.clone(),
            None => key.to_owned(),
        }
    }

    pub fn traitname(&self, owner: &str, name: &str) -> String {
        let count = self.interfaces.keys().filter(|(_, n)| n == name).count();
        match count <= 1 {
            true => name.to_owned(),
            false => format!("{}${name}", self.prefix(owner)),
        }
    }

    pub fn target(&self, stem: &str, alias: &str) -> String {
        self.imports
            .get(&(stem.to_owned(), alias.to_owned()))
            .map_or_else(|| alias.to_owned(), |(leaf, _)| leaf.clone())
    }

    pub fn unalias(&self, stem: &str, name: &str) -> (String, Spec) {
        let mut current = (stem.to_owned(), name.to_owned());
        for _ in 0..8 {
            let Some(alias) = self.aliases.get(&current) else {
                break;
            };
            if !alias.generics.is_empty() {
                break;
            }
            match &alias.target {
                Spec::Named {
                    module: Some(m),
                    name: n,
                    arguments,
                } if arguments.is_empty() => current = (self.target(&current.0, m), n.clone()),
                Spec::Named {
                    module: None,
                    name: n,
                    arguments,
                } if arguments.is_empty() => current = (current.0.clone(), n.clone()),
                other => return (current.0, other.clone()),
            }
        }
        (
            current.0,
            Spec::Named {
                module: None,
                name: current.1,
                arguments: Vec::new(),
            },
        )
    }

    pub fn unique(&self, name: &str) -> bool {
        let s = self.structs.keys().filter(|(_, n)| n == name).count();
        let e = self.enums.keys().filter(|(_, n)| n == name).count();
        s + e <= 1
    }
}
