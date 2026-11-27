use crate::Fault;
use crate::eval::asset::Pool;
use crate::eval::constant::Evaluator;
use crate::front::loader::Bundle;
use crate::front::numeric::{Value, retype};
use crate::front::types::{Map, Typing};
use crate::front::world::World;
use crate::layout::Type;
use crate::tree::{Import, Routine, Table, Unit};
use simplex::tree as syntax;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

pub struct Task {
    pub stem: String,
    pub name: String,
    pub symbol: String,
    pub subst: Map,
    pub constants: BTreeMap<String, Value>,
    pub exported: bool,
}

pub struct Spec<'a> {
    pub expr: &'a syntax::Expression,
    pub subst: &'a Map,
    pub constants: &'a BTreeMap<String, Value>,
    pub exported: bool,
}

pub struct Engine<'a> {
    pub ns: String,
    pub root: PathBuf,
    pub shared: bool,
    pub world: &'a World,
    pub typing: Typing<'a>,
    pub pool: Pool,
    pub globals: BTreeMap<(String, String), Value>,
    pub embedded: BTreeMap<(String, String), String>,
    pub libraries: BTreeSet<String>,
    pub imports: BTreeMap<String, Import>,
    pub tables: BTreeMap<String, Table>,
    pub drops: BTreeMap<String, Option<(String, bool)>>,
    pub routines: Vec<Routine>,
    pub seen: BTreeSet<String>,
    pub pending: VecDeque<Task>,
    pub counter: usize,
    pub fault: Option<Fault>,
}

pub fn compile(ns: &str, bundle: &Bundle, shared: bool) -> Result<Unit, Fault> {
    let world = World::build(bundle);
    let mut engine = Engine {
        ns: ns.to_owned(),
        root: bundle.root.clone(),
        shared,
        world: &world,
        typing: Typing::new(&world),
        pool: Pool::default(),
        globals: BTreeMap::new(),
        embedded: BTreeMap::new(),
        libraries: BTreeSet::new(),
        imports: BTreeMap::new(),
        tables: BTreeMap::new(),
        drops: BTreeMap::new(),
        routines: Vec::new(),
        seen: BTreeSet::new(),
        pending: VecDeque::new(),
        counter: 0,
        fault: None,
    };
    engine.prepare()?;
    engine.boundaries();
    engine.conformances();
    engine.declarations();
    while let Some(task) = engine.pending.pop_front() {
        engine.specialize(task);
    }
    if let Some(err) = engine.fault {
        return Err(err);
    }
    Ok(engine.finish())
}

impl<'a> Engine<'a> {
    pub fn fresh(&mut self, prefix: &str) -> String {
        let id = self.counter;
        self.counter += 1;
        format!("${prefix}{id}")
    }

    pub fn symbol(&self, key: &str, name: &str) -> String {
        self.world.symbol(&self.ns, key, name)
    }

    pub fn vtable(&self, key: &str, subject: &str, contract: &str) -> String {
        self.world.table(&self.ns, key, subject, contract)
    }

    pub fn coords<'b>(&'b self, key: &'b str) -> (&'b str, &'b str) {
        self.world.coords(&self.ns, key)
    }

    fn prepare(&mut self) -> Result<(), Fault> {
        for ((stem, name), s) in &self.world.structs {
            if s.generics.is_empty() {
                self.typing.nominal(stem, name, &[]);
            }
        }
        for ((stem, name), e) in &self.world.enums {
            if e.generics.is_empty() {
                self.typing.nominal(stem, name, &[]);
            }
        }
        for stem in &self.world.order {
            self.evaluate(stem)?;
        }
        Ok(())
    }

    fn evaluate(&mut self, stem: &str) -> Result<(), Fault> {
        let Some(items) = self.world.programs.get(stem) else {
            return Ok(());
        };
        for item in items {
            match &item.statement {
                syntax::Statement::Bind(b) => self.constant(stem, b)?,
                syntax::Statement::Evaluate(syntax::Expression::Meta(block)) => {
                    let mut eval = self.evaluator(stem, Map::new());
                    eval.block(block);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn constant(&mut self, stem: &str, b: &syntax::Binding) -> Result<(), Fault> {
        let (ns, module) = self.world.coords(&self.ns, stem);
        if let syntax::Expression::Text(raw) = &b.value {
            let bytes = crate::eval::asset::unescape(raw);
            let sym = self.pool.intern((ns, module), bytes);
            self.embedded.insert((stem.to_owned(), b.name.clone()), sym);
            return Ok(());
        }
        if let syntax::Expression::Call {
            callee, arguments, ..
        } = &b.value
            && matches!(callee.as_ref(), syntax::Expression::Word { text, .. } if text == "embed")
            && let Some(syntax::Expression::Text(raw)) = arguments.first()
        {
            let scope = self.world.folder(&self.root, stem);
            let sym = self.pool.embed((ns, module), scope, raw)?;
            self.embedded.insert((stem.to_owned(), b.name.clone()), sym);
            return Ok(());
        }
        if let syntax::Expression::Meta(block) = &b.value {
            let mut eval = self.evaluator(stem, Map::new());
            let mut val = eval.block(block);
            if let Some(spec) = &b.annotation {
                let kind = self.typing.resolve(stem, spec, &Map::new());
                val = retype(val, kind);
            }
            self.globals.insert((stem.to_owned(), b.name.clone()), val);
        }
        Ok(())
    }

    pub fn evaluator(&mut self, stem: &str, subst: Map) -> Evaluator<'a, '_> {
        Evaluator {
            world: self.world,
            typing: &mut self.typing,
            stem: stem.to_owned(),
            globals: self.globals.clone(),
            vars: BTreeMap::new(),
            subst,
        }
    }

    fn finish(self) -> Unit {
        let last = self.world.order.last().cloned().unwrap_or_default();
        let main = self.symbol(&last, "main");
        let found = self.routines.iter().any(|r| r.symbol == main);
        let entry = (!self.shared && found).then_some(main);
        let _ = Type::Void;
        Unit {
            namespace: self.ns,
            shared: self.shared,
            libraries: self.libraries.into_iter().collect(),
            imports: self.imports.into_values().collect(),
            assets: self.pool.items,
            tables: self.tables.into_values().collect(),
            catalog: self.typing.catalog,
            routines: self.routines,
            entry,
        }
    }
}
