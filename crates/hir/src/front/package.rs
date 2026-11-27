use crate::Fault;
use crate::front::access::{Exports, summarize, verify};
use crate::front::loader::{Bundle, Dynamic, Meta, Source, filename, stemname};
use crate::plan::{Graph, Linked};
use simplex::tree::{Import, Statement};
use simplex::{Code, Finding};
use simplex_package::Coordinate;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub type Requested = Vec<(Coordinate, (usize, usize))>;

struct Linker<'a> {
    graph: &'a Graph,
    root: Coordinate,
    building: &'a BTreeSet<Coordinate>,
    active: BTreeSet<(Coordinate, PathBuf)>,
    seen: BTreeMap<(Coordinate, PathBuf), (String, Exports)>,
    units: Vec<Source>,
    shared: BTreeMap<String, Dynamic>,
    requested: Requested,
}

pub fn load(
    graph: &Graph,
    target: &Linked,
    building: &BTreeSet<Coordinate>,
) -> Result<(Bundle, Requested), Fault> {
    let entry = target.root.join(&target.entry);
    if !entry.is_file() {
        return Err(Fault::Io(entry.display().to_string()));
    }
    let canonical = canonicalize(&entry);
    let mut linker = Linker {
        graph,
        root: target.coordinate.clone(),
        building,
        active: BTreeSet::new(),
        seen: BTreeMap::new(),
        units: Vec::new(),
        shared: BTreeMap::new(),
        requested: Vec::new(),
    };
    linker.visit(target, &canonical, (1, 1))?;
    Ok((
        Bundle {
            root: target.root.clone(),
            units: linker.units,
            shared: linker.shared,
        },
        linker.requested,
    ))
}

impl<'a> Linker<'a> {
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
        if let [alias] = imp.path.as_slice()
            && let Some(coord) = package.dependencies.get(alias)
        {
            let target = self
                .graph
                .lookup(coord)
                .ok_or_else(|| fault(Code::Import, pos))?;
            return match imp.dynamic {
                true => self.dynamic(target, pos),
                false => self.external(target, pos),
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
        pos: (usize, usize),
    ) -> Result<(String, Exports), Fault> {
        if target.coordinate != self.root && self.building.contains(&target.coordinate) {
            return Err(fault(Code::Cycle, pos));
        }
        let entry = target.root.join(&target.entry);
        if !entry.is_file() {
            return Err(fault(Code::Import, pos));
        }
        self.visit(target, &entry, pos)
    }

    fn dynamic(
        &mut self,
        target: &Linked,
        pos: (usize, usize),
    ) -> Result<(String, Exports), Fault> {
        if self.building.contains(&target.coordinate) {
            return Err(fault(Code::Cycle, pos));
        }
        let entry = target.root.join(&target.entry);
        if !entry.is_file() {
            return Err(fault(Code::Import, pos));
        }
        self.requested.push((target.coordinate.clone(), pos));
        let key = format!(
            "dylib${}${}",
            target.coordinate.namespace(),
            stemname(Path::new(&target.entry))
        );
        if let Some(existing) = self.shared.get(&key) {
            return Ok((key, summarize(&existing.entry.program, true)));
        }
        let (sub, _) = load(self.graph, target, self.building)?;
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
        let library = format!("lib{}.so", parsed.meta.module);
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
    let mut rel = PathBuf::new();
    for seg in &imp.path {
        rel.push(seg);
    }
    rel.set_extension("sx");
    let direct = dir.join(&rel);
    if direct.is_file() {
        return Ok(direct);
    }
    let rooted = package.root.join(&rel);
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
    let module = modulename(package, path);
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

fn modulename(package: &Linked, path: &Path) -> String {
    let entry = canonicalize(&package.root.join(&package.entry));
    if path == entry {
        return stemname(Path::new(&package.entry));
    }
    let base = canonicalize(&package.root);
    let Ok(rel) = path.strip_prefix(&base) else {
        return stemname(path);
    };
    let trimmed = rel.with_extension("");
    let parts: Vec<&str> = trimmed
        .components()
        .filter_map(|comp| match comp {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect();
    match parts.is_empty() {
        true => stemname(path),
        false => parts.join("$"),
    }
}

fn canonicalize(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn fault(code: Code, pos: (usize, usize)) -> Fault {
    Fault::Frontend(Finding {
        code,
        line: pos.0,
        column: pos.1,
    })
}
