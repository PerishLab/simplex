use crate::Fault;
use simplex::tree::{Import, Program, Statement};
use simplex::{Code, Finding};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Meta {
    pub ns: String,
    pub module: String,
    pub root: PathBuf,
    pub strict: bool,
    pub local: bool,
    pub qualified: bool,
}

#[derive(Clone, Debug)]
pub struct Source {
    pub key: String,
    pub file: String,
    pub meta: Meta,
    pub imports: BTreeMap<String, (String, bool)>,
    pub program: Program,
}

#[derive(Clone, Debug)]
pub struct Dynamic {
    pub library: String,
    pub entry: Source,
    pub support: Vec<Source>,
}

#[derive(Clone, Debug)]
pub struct Bundle {
    pub root: PathBuf,
    pub units: Vec<Source>,
    pub shared: BTreeMap<String, Dynamic>,
}

struct Walk {
    ns: String,
    root: PathBuf,
    active: BTreeSet<PathBuf>,
    seen: BTreeSet<PathBuf>,
    units: Vec<Source>,
    shared: BTreeMap<String, Dynamic>,
}

pub fn load(ns: &str, entry: &Path) -> Result<Bundle, Fault> {
    if !entry.is_file() {
        return Err(Fault::Io(entry.display().to_string()));
    }
    let canonical = fs::canonicalize(entry).unwrap_or_else(|_| entry.to_path_buf());
    let root = anchor(&canonical);
    let mut walk = Walk {
        ns: ns.to_owned(),
        root: root.clone(),
        active: BTreeSet::new(),
        seen: BTreeSet::new(),
        units: Vec::new(),
        shared: BTreeMap::new(),
    };
    walk.visit(&canonical, (1, 1))?;
    Ok(Bundle {
        root,
        units: walk.units,
        shared: walk.shared,
    })
}

impl Walk {
    fn visit(&mut self, target: &Path, pos: (usize, usize)) -> Result<String, Fault> {
        let key = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
        let stem = stemname(&key);
        if self.seen.contains(&key) {
            return Ok(stem);
        }
        if !self.active.insert(key.clone()) {
            return Err(Fault::Frontend(Finding {
                code: Code::Cycle,
                line: pos.0,
                column: pos.1,
            }));
        }
        let mut parsed = parse(&self.ns, &self.root, &key)?;
        let fallback = self.root.clone();
        let dir = key.parent().unwrap_or(&fallback);
        self.dependencies(dir, &mut parsed)?;
        self.active.remove(&key);
        self.seen.insert(key);
        self.units.push(parsed);
        Ok(stem)
    }

    fn dependencies(&mut self, dir: &Path, current: &mut Source) -> Result<(), Fault> {
        for item in &current.program.items {
            let Statement::Import(imp) = &item.statement else {
                continue;
            };
            let pos = (item.line, item.column);
            let target = self.locate(dir, imp, pos)?;
            match imp.dynamic {
                true => {
                    let parsed = parse(&self.ns, &self.root, &target)?;
                    let name = parsed.key.clone();
                    let library = format!("lib{name}.so");
                    current
                        .imports
                        .insert(imp.name.clone(), (name.clone(), true));
                    self.shared.entry(name).or_insert(Dynamic {
                        library,
                        entry: parsed,
                        support: Vec::new(),
                    });
                }
                false => {
                    let resolved = self.visit(&target, pos)?;
                    current.imports.insert(imp.name.clone(), (resolved, false));
                }
            }
        }
        Ok(())
    }

    fn locate(&self, dir: &Path, imp: &Import, pos: (usize, usize)) -> Result<PathBuf, Fault> {
        let mut rel = PathBuf::new();
        for seg in &imp.path {
            rel.push(seg);
        }
        rel.set_extension("sx");
        let direct = dir.join(&rel);
        if direct.is_file() {
            return Ok(direct);
        }
        let rooted = self.root.join(&rel);
        if rooted.is_file() {
            return Ok(rooted);
        }
        if let Some(parent) = dir.parent() {
            let sibling = parent.join("pass").join(&rel);
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
        Err(Fault::Frontend(Finding {
            code: Code::Import,
            line: pos.0,
            column: pos.1,
        }))
    }
}

fn parse(ns: &str, root: &Path, path: &Path) -> Result<Source, Fault> {
    let text = fs::read_to_string(path).map_err(|_| Fault::Io(path.display().to_string()))?;
    let file = filename(path);
    let stem = stemname(path);
    let program = simplex::inspect(Some(&file), path.parent(), &text).map_err(Fault::Frontend)?;
    Ok(Source {
        key: stem.clone(),
        file,
        meta: Meta {
            ns: ns.to_owned(),
            module: stem,
            root: root.to_path_buf(),
            strict: false,
            local: true,
            qualified: false,
        },
        imports: BTreeMap::new(),
        program,
    })
}

pub fn filename(path: &Path) -> String {
    path.file_name()
        .and_then(|part| part.to_str())
        .unwrap_or("main.sx")
        .to_owned()
}

pub fn stemname(path: &Path) -> String {
    path.file_stem()
        .and_then(|part| part.to_str())
        .unwrap_or("main")
        .to_owned()
}

fn anchor(entry: &Path) -> PathBuf {
    entry
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}
