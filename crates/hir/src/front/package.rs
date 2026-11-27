use crate::Fault;
use crate::front::access::{Exports, summarize, verify};
use crate::front::loader::{Bundle, Dynamic, Folder, Meta, Source, canonicalize, filename};
use crate::plan::{Graph, Linked};
use simplex::tree::{Import, Statement};
use simplex::{Code, Finding};
use simplex_package::Coordinate;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub type Requested = Vec<(Coordinate, (usize, usize))>;

struct Linker<'a> {
    graph: &'a Graph,
    root: Coordinate,
    building: &'a BTreeSet<Coordinate>,
    checked: BTreeSet<Coordinate>,
    active: BTreeSet<(Coordinate, PathBuf)>,
    seen: BTreeMap<(Coordinate, PathBuf), (String, Exports)>,
    units: Vec<Source>,
    shared: BTreeMap<String, Dynamic>,
    requested: Requested,
}

pub fn load(
    graph: &Graph,
    target: &Linked,
    shared: bool,
    building: &BTreeSet<Coordinate>,
) -> Result<(Bundle, Requested), Fault> {
    match shared {
        false => {
            let entry = target.root.join("main.sx");
            if !entry.is_file() {
                return Err(Fault::Io(entry.display().to_string()));
            }
            walkfiles(graph, target, &[entry], building)
        }
        true => {
            let files = Folder(&target.root).discover()?;
            walkfiles(graph, target, &files, building)
        }
    }
}

fn walkfiles(
    graph: &Graph,
    target: &Linked,
    files: &[PathBuf],
    building: &BTreeSet<Coordinate>,
) -> Result<(Bundle, Requested), Fault> {
    let mut linker = Linker {
        graph,
        root: target.coordinate.clone(),
        building,
        checked: BTreeSet::new(),
        active: BTreeSet::new(),
        seen: BTreeMap::new(),
        units: Vec::new(),
        shared: BTreeMap::new(),
        requested: Vec::new(),
    };
    linker.checkshadow(target)?;
    for file in files {
        let canonical = canonicalize(file);
        linker.visit(target, &canonical, (1, 1))?;
    }
    let bundle = Bundle {
        root: target.root.clone(),
        units: linker.units,
        shared: linker.shared,
    };
    Ok((bundle, linker.requested))
}

impl<'a> Linker<'a> {
    fn checkshadow(&mut self, package: &Linked) -> Result<(), Fault> {
        if !self.checked.insert(package.coordinate.clone()) {
            return Ok(());
        }
        if Folder(&package.root).shadowed(&package.dependencies)? {
            return Err(fault(Code::Import, (1, 1)));
        }
        Ok(())
    }

    fn visit(
        &mut self,
        package: &Linked,
        path: &Path,
        pos: (usize, usize),
    ) -> Result<(String, Exports), Fault> {
        let canonical = canonicalize(path);
        let token = (package.coordinate.clone(), canonical.clone());
        if let Some(found) = self.seen.get(&token) {
            return Ok(found.clone());
        }
        if !self.active.insert(token.clone()) {
            return Err(fault(Code::Cycle, pos));
        }
        let mut parsed = read(package, &canonical, package.coordinate == self.root, pos)?;
        let dir = canonical.parent().unwrap_or(&package.root);
        let modules = self.dependencies(package, dir, &mut parsed)?;
        verify(&modules, &parsed.program).map_err(Fault::Frontend)?;
        let exports = summarize(&parsed.program, false);
        let key = parsed.key.clone();
        self.active.remove(&token);
        self.seen.insert(token, (key.clone(), exports.clone()));
        self.units.push(parsed);
        Ok((key, exports))
    }

    fn dependencies(
        &mut self,
        package: &Linked,
        dir: &Path,
        current: &mut Source,
    ) -> Result<BTreeMap<String, Exports>, Fault> {
        let mut modules = BTreeMap::new();
        for item in &current.program.items {
            let Statement::Import(imp) = &item.statement else {
                continue;
            };
            let pos = (item.line, item.column);
            let (key, exports) = self.resolve((package, dir), imp, pos)?;
            current.imports.insert(imp.name.clone(), (key, imp.dynamic));
            modules.insert(imp.name.clone(), exports);
        }
        Ok(modules)
    }

    fn resolve(
        &mut self,
        scope: (&Linked, &Path),
        imp: &Import,
        pos: (usize, usize),
    ) -> Result<(String, Exports), Fault> {
        let (package, dir) = scope;
        if let Some(alias) = imp.path.first()
            && let Some(coord) = package.dependencies.get(alias)
        {
            if imp.path.len() == 1 {
                let code = match imp.dynamic {
                    true => Code::Boundary,
                    false => Code::Import,
                };
                return Err(fault(code, pos));
            }
            let target = self
                .graph
                .lookup(coord)
                .ok_or_else(|| fault(Code::Import, pos))?;
            let subpath = &imp.path[1..];
            return match imp.dynamic {
                true => self.dynamic(target, subpath, pos),
                false => self.external(target, subpath, pos),
            };
        }
        let local = locate(package, dir, imp, pos)?;
        match imp.dynamic {
            true => self.localshared(package, &local, pos),
            false => self.visit(package, &local, pos),
        }
    }

    fn external(
        &mut self,
        target: &Linked,
        subpath: &[String],
        pos: (usize, usize),
    ) -> Result<(String, Exports), Fault> {
        if target.coordinate != self.root && self.building.contains(&target.coordinate) {
            return Err(fault(Code::Cycle, pos));
        }
        self.checkshadow(target)?;
        let entry = Folder(&target.root).resolve(subpath);
        if !entry.is_file() {
            return Err(fault(Code::Import, pos));
        }
        self.visit(target, &entry, pos)
    }

    fn dynamic(
        &mut self,
        target: &Linked,
        subpath: &[String],
        pos: (usize, usize),
    ) -> Result<(String, Exports), Fault> {
        if self.building.contains(&target.coordinate) {
            return Err(fault(Code::Cycle, pos));
        }
        self.checkshadow(target)?;
        let entry = Folder(&target.root).resolve(subpath);
        if !entry.is_file() {
            return Err(fault(Code::Import, pos));
        }
        self.requested.push((target.coordinate.clone(), pos));
        let canonical = canonicalize(&entry);
        let module = Folder(&target.root).module(&canonical);
        let key = format!("dylib${}${}", target.coordinate.namespace(), module);
        if let Some(existing) = self.shared.get(&key) {
            return Ok((key, summarize(&existing.entry.program, true)));
        }
        let (sub, _) = walkfiles(self.graph, target, &[canonical], self.building)?;
        let mut units = sub.units;
        let Some(mut main) = units.pop() else {
            return Err(fault(Code::Import, pos));
        };
        main.key.clone_from(&key);
        let exports = summarize(&main.program, true);
        self.shared.insert(
            key.clone(),
            Dynamic {
                library: target.coordinate.stem(),
                entry: main,
                support: units,
            },
        );
        Ok((key, exports))
    }

    fn localshared(
        &mut self,
        package: &Linked,
        path: &Path,
        pos: (usize, usize),
    ) -> Result<(String, Exports), Fault> {
        let mut parsed = read(package, path, false, pos)?;
        let key = format!("dylib${}", parsed.key);
        let library = format!("{}.so", parsed.meta.module);
        parsed.key.clone_from(&key);
        let exports = summarize(&parsed.program, true);
        self.shared.entry(key.clone()).or_insert(Dynamic {
            library,
            entry: parsed,
            support: Vec::new(),
        });
        Ok((key, exports))
    }
}

fn locate(
    package: &Linked,
    dir: &Path,
    imp: &Import,
    pos: (usize, usize),
) -> Result<PathBuf, Fault> {
    let direct = Folder(dir).resolve(&imp.path);
    if direct.is_file() {
        return Ok(direct);
    }
    let rooted = Folder(&package.root).resolve(&imp.path);
    if rooted.is_file() {
        return Ok(rooted);
    }
    Err(fault(Code::Import, pos))
}

fn read(package: &Linked, path: &Path, local: bool, pos: (usize, usize)) -> Result<Source, Fault> {
    let text = fs::read_to_string(path).map_err(|_| fault(Code::Import, pos))?;
    let file = filename(path);
    let program = simplex::inspect(Some(&file), None, &text).map_err(Fault::Frontend)?;
    let ns = package.coordinate.namespace();
    let module = Folder(&package.root).module(path);
    let key = format!("{ns}${module}");
    Ok(Source {
        key,
        file,
        meta: Meta {
            ns,
            module,
            root: package.root.clone(),
            strict: true,
            local,
            qualified: true,
        },
        imports: BTreeMap::new(),
        program,
    })
}

fn fault(code: Code, pos: (usize, usize)) -> Fault {
    Fault::Frontend(Finding {
        code,
        line: pos.0,
        column: pos.1,
    })
}
