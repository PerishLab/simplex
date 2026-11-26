use crate::Fault;
use simplex::tree::{Import, Program, Statement};
use simplex::{Code, Finding};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Source {
    pub stem: String,
    pub file: String,
    pub program: Program,
}

#[derive(Clone, Debug)]
pub struct Bundle {
    pub root: PathBuf,
    pub units: Vec<Source>,
    pub shared: BTreeMap<String, Source>,
}

struct Walk {
    root: PathBuf,
    active: BTreeSet<PathBuf>,
    seen: BTreeSet<PathBuf>,
    units: Vec<Source>,
    shared: BTreeMap<String, Source>,
}

pub fn load(entry: &Path) -> Result<Bundle, Fault> {
    if !entry.is_file() {
        return Err(Fault::Io(entry.display().to_string()));
    }
    let canonical = fs::canonicalize(entry).unwrap_or_else(|_| entry.to_path_buf());
    let root = anchor(&canonical);
    let mut walk = Walk {
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
    fn visit(&mut self, target: &Path, pos: (usize, usize)) -> Result<(), Fault> {
        let key = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
        if self.seen.contains(&key) {
            return Ok(());
        }
        if !self.active.insert(key.clone()) {
            return Err(Fault::Frontend(Finding {
                code: Code::Cycle,
                line: pos.0,
                column: pos.1,
            }));
        }
        let parsed = parse(&key)?;
        let fallback = self.root.clone();
        let dir = key.parent().unwrap_or(&fallback);
        self.dependencies(dir, &parsed)?;
        self.active.remove(&key);
        self.seen.insert(key);
        self.units.push(parsed);
        Ok(())
    }

    fn dependencies(&mut self, dir: &Path, current: &Source) -> Result<(), Fault> {
        for item in &current.program.items {
            let Statement::Import(imp) = &item.statement else {
                continue;
            };
            let pos = (item.line, item.column);
            let target = self.locate(dir, imp, pos)?;
            match imp.dynamic {
                true => {
                    let parsed = parse(&target)?;
                    self.shared.entry(parsed.stem.clone()).or_insert(parsed);
                }
                false => self.visit(&target, pos)?,
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

fn parse(path: &Path) -> Result<Source, Fault> {
    let text = fs::read_to_string(path).map_err(|_| Fault::Io(path.display().to_string()))?;
    let file = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("main.sx")
        .to_owned();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main")
        .to_owned();
    let program = simplex::inspect(Some(&file), path.parent(), &text).map_err(Fault::Frontend)?;
    Ok(Source {
        stem,
        file,
        program,
    })
}

fn anchor(entry: &Path) -> PathBuf {
    entry
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}
