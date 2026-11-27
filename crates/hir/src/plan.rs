use crate::Fault;
use crate::bridge::bridge;
use crate::front::package::load;
use crate::lower::engine::compile;
use simplex::{Code, Finding};
use simplex_ir::Module;
use simplex_package::Coordinate;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Linked {
    pub coordinate: Coordinate,
    pub root: PathBuf,
    pub entry: String,
    pub dependencies: BTreeMap<String, Coordinate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub root: Linked,
    pub packages: BTreeMap<Coordinate, Linked>,
}

impl Graph {
    pub(crate) fn lookup(&self, coordinate: &Coordinate) -> Option<&Linked> {
        if *coordinate == self.root.coordinate {
            return Some(&self.root);
        }
        self.packages.get(coordinate)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub namespace: String,
    pub stem: String,
    pub module: Module,
    pub dynamics: Vec<(Coordinate, Module)>,
}

struct Assembler<'a> {
    graph: &'a Graph,
    building: BTreeSet<Coordinate>,
    built: BTreeSet<Coordinate>,
    dynamics: Vec<(Coordinate, Module)>,
}

pub fn build(graph: &Graph, shared: bool) -> Result<Plan, Fault> {
    let mut assembler = Assembler {
        graph,
        building: BTreeSet::new(),
        built: BTreeSet::new(),
        dynamics: Vec::new(),
    };
    let module = assembler.compile(&graph.root, shared, (1, 1))?;
    let namespace = graph.root.coordinate.namespace();
    let stem = match shared {
        true => graph.root.coordinate.stem(),
        false => graph.root.coordinate.name.clone(),
    };
    Ok(Plan {
        namespace,
        stem,
        module,
        dynamics: assembler.dynamics,
    })
}

impl<'a> Assembler<'a> {
    fn compile(
        &mut self,
        target: &Linked,
        shared: bool,
        pos: (usize, usize),
    ) -> Result<Module, Fault> {
        if !self.building.insert(target.coordinate.clone()) {
            return Err(Fault::Frontend(Finding {
                code: Code::Cycle,
                line: pos.0,
                column: pos.1,
            }));
        }
        let (bundle, requested) = load(self.graph, target, &self.building)?;
        for (coordinate, site) in requested {
            self.dependency(&coordinate, site)?;
        }
        let namespace = target.coordinate.namespace();
        let unit = compile(&namespace, &bundle, shared)?;
        self.building.remove(&target.coordinate);
        Ok(bridge(&unit))
    }

    fn dependency(&mut self, coordinate: &Coordinate, pos: (usize, usize)) -> Result<(), Fault> {
        if self.building.contains(coordinate) {
            return Err(Fault::Frontend(Finding {
                code: Code::Cycle,
                line: pos.0,
                column: pos.1,
            }));
        }
        if self.built.contains(coordinate) {
            return Ok(());
        }
        let target = self
            .graph
            .lookup(coordinate)
            .ok_or(Fault::Frontend(Finding {
                code: Code::Import,
                line: pos.0,
                column: pos.1,
            }))?;
        let module = self.compile(target, true, pos)?;
        self.built.insert(coordinate.clone());
        self.dynamics.push((coordinate.clone(), module));
        Ok(())
    }
}
