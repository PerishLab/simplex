use crate::front::numeric::{encode, primitive};
use crate::front::world::World;
use crate::layout::{Catalog, Field, Footprint, Layout, Type};
use simplex::tree::{Generic, Spec, Structure};
use std::collections::BTreeMap;

pub type Map = BTreeMap<String, Type>;

pub struct Typing<'a> {
    pub world: &'a World,
    pub catalog: Catalog,
    pub owners: BTreeMap<String, (String, String, Vec<Type>)>,
}

impl<'a> Typing<'a> {
    pub fn new(world: &'a World) -> Self {
        Self {
            world,
            catalog: Catalog::default(),
            owners: BTreeMap::new(),
        }
    }

    pub fn origin(&mut self) -> Type {
        let key = String::from("Origin");
        if !self.catalog.records.contains_key(&key) {
            let slice = Type::Slice(Box::new(Type::Unsigned(1)));
            let fields = vec![
                field("package", 0, slice.clone()),
                field("module", 16, slice.clone()),
                field("file", 32, slice),
                field("line", 48, Type::Unsigned(8)),
                field("column", 56, Type::Unsigned(8)),
            ];
            let layout = Layout {
                name: key.clone(),
                footprint: Footprint::aggregate(64, 8),
                fields,
            };
            self.catalog.records.insert(key.clone(), layout);
        }
        Type::Record(key)
    }

    pub fn owner(&self, key: &str) -> Option<&(String, String, Vec<Type>)> {
        self.owners.get(key)
    }

    pub fn optional(&mut self, stem: &str, spec: Option<&Spec>, subst: &Map) -> Type {
        match spec {
            Some(inner) => self.resolve(stem, inner, subst),
            None => Type::Void,
        }
    }

    pub fn resolve(&mut self, stem: &str, spec: &Spec, subst: &Map) -> Type {
        match spec {
            Spec::Slice(inner) => Type::Slice(Box::new(self.resolve(stem, inner, subst))),
            Spec::Borrow { target, .. } => self.borrowed(stem, target, subst),
            Spec::Callable {
                asynchronous,
                inputs,
                output,
            } => {
                let params = inputs
                    .iter()
                    .map(|i| self.resolve(stem, i, subst))
                    .collect();
                let raw = self.optional(stem, output.as_deref(), subst);
                let out = match asynchronous {
                    true => Type::Fiber(Box::new(raw)),
                    false => raw,
                };
                Type::Function {
                    inputs: params,
                    output: Box::new(out),
                }
            }
            Spec::Named {
                module,
                name,
                arguments,
            } => self.named(stem, module.as_deref(), (name, arguments), subst),
        }
    }

    fn borrowed(&mut self, stem: &str, target: &Spec, subst: &Map) -> Type {
        match target {
            Spec::Slice(elem) => Type::Slice(Box::new(self.resolve(stem, elem, subst))),
            Spec::Named {
                module,
                name,
                arguments,
            } if arguments.is_empty() => {
                if let Some(bound) = subst.get(name) {
                    return Type::Pointer(Box::new(bound.clone()));
                }
                let owner = module
                    .as_deref()
                    .map_or_else(|| stem.to_owned(), |m| self.world.target(stem, m));
                let (real, resolved) = self.world.unalias(&owner, name);
                let interfaces = &self.world.interfaces;
                if let Spec::Named { name: found, .. } = &resolved
                    && interfaces.contains_key(&(real.clone(), found.clone()))
                {
                    return Type::Trait(self.world.traitname(&real, found));
                }
                let plain = Spec::Named {
                    module: module.clone(),
                    name: name.clone(),
                    arguments: Vec::new(),
                };
                Type::Pointer(Box::new(self.resolve(stem, &plain, subst)))
            }
            other => Type::Pointer(Box::new(self.resolve(stem, other, subst))),
        }
    }

    fn named(
        &mut self,
        stem: &str,
        module: Option<&str>,
        pair: (&str, &[Spec]),
        subst: &Map,
    ) -> Type {
        let (name, arguments) = pair;
        if module.is_none() {
            if let Some(found) = subst.get(name) {
                return found.clone();
            }
            if let Some(prim) = primitive(name) {
                return prim;
            }
            if let Some(builtin) = self.container(stem, name, arguments, subst) {
                return builtin;
            }
            if name == "Origin" {
                return self.origin();
            }
        }
        let owner = module.map_or_else(|| stem.to_owned(), |m| self.world.target(stem, m));
        let aliases = &self.world.aliases;
        if let Some(alias) = aliases.get(&(owner.clone(), name.to_owned())).cloned() {
            let next = bind(&alias.generics, arguments, (stem, subst), self);
            return self.resolve(&owner, &alias.target, &next);
        }
        let args: Vec<Type> = arguments
            .iter()
            .map(|a| self.resolve(stem, a, subst))
            .collect();
        self.nominal(&owner, name, &args)
    }

    fn container(&mut self, stem: &str, name: &str, args: &[Spec], subst: &Map) -> Option<Type> {
        let [first] = args else {
            return None;
        };
        match name {
            "Slice" => Some(Type::Slice(Box::new(self.resolve(stem, first, subst)))),
            "List" => Some(Type::List(Box::new(self.resolve(stem, first, subst)))),
            "Fiber" => Some(Type::Fiber(Box::new(self.resolve(stem, first, subst)))),
            _ => None,
        }
    }

    pub fn nominal(&mut self, owner: &str, name: &str, args: &[Type]) -> Type {
        let pair = (owner.to_owned(), name.to_owned());
        let structs = &self.world.structs;
        if let Some(def) = structs.get(&pair).cloned() {
            let key = self.label(owner, name, args);
            if !self.catalog.records.contains_key(&key) {
                self.owners.insert(
                    key.clone(),
                    (owner.to_owned(), name.to_owned(), args.to_vec()),
                );
                let layout = self.structure(&key, owner, &def, args);
                self.catalog.records.insert(key.clone(), layout);
            }
            return Type::Record(key);
        }
        if let Some(choice) = self.choice(owner, name, args) {
            return choice;
        }
        let interfaces = &self.world.interfaces;
        if interfaces.contains_key(&pair) {
            return Type::Trait(self.world.traitname(owner, name));
        }
        let key = self.label(owner, name, args);
        self.catalog.records.entry(key.clone()).or_insert(Layout {
            name: key.clone(),
            footprint: Footprint::scalar(8),
            fields: Vec::new(),
        });
        Type::Record(key)
    }

    pub fn label(&self, owner: &str, name: &str, args: &[Type]) -> String {
        let base = match self.world.unique(name) {
            true => name.to_owned(),
            false => format!("{}${name}", self.world.prefix(owner)),
        };
        match args.is_empty() {
            true => base,
            false => {
                let parts: Vec<String> = args.iter().map(encode).collect();
                format!("{base}${}", parts.join("$"))
            }
        }
    }

    fn structure(&mut self, key: &str, owner: &str, def: &Structure, args: &[Type]) -> Layout {
        let local = zip(&def.generics, args);
        let (mut offset, mut align, mut fields) = (0u64, 1u64, Vec::new());
        for slot in &def.fields {
            let kind = self.resolve(owner, &slot.spec, &local);
            let fp = kind.footprint(&self.catalog);
            let step = fp.align.max(1);
            offset = pad(offset, step);
            fields.push(field(&slot.name, offset, kind));
            offset += fp.size;
            align = align.max(step);
        }
        let size = match offset {
            0 => 0,
            _ => pad(offset, align),
        };
        Layout {
            name: key.to_owned(),
            footprint: Footprint::aggregate(size, align),
            fields,
        }
    }
}

pub fn field(name: &str, offset: u64, kind: Type) -> Field {
    Field {
        name: name.to_owned(),
        offset,
        kind,
    }
}

fn bind(generics: &[Generic], args: &[Spec], caller: (&str, &Map), typing: &mut Typing<'_>) -> Map {
    generics
        .iter()
        .zip(args)
        .map(|(g, a)| (g.name.clone(), typing.resolve(caller.0, a, caller.1)))
        .collect()
}

pub fn zip(generics: &[Generic], args: &[Type]) -> Map {
    generics
        .iter()
        .zip(args)
        .map(|(g, t)| (g.name.clone(), t.clone()))
        .collect()
}

pub fn pad(offset: u64, align: u64) -> u64 {
    let step = align.max(1);
    (offset + step - 1) & !(step - 1)
}
