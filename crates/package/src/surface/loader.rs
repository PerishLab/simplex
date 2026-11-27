use crate::{Fault, Manifest};
use simplex::Program;
use simplex::tree::{Import, Statement};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) type Links = BTreeMap<String, String>;

pub(crate) struct Unit {
    pub(crate) id: String,
    pub(crate) program: Program,
    pub(crate) locals: Links,
    pub(crate) external: Links,
}

pub(crate) struct Graph {
    pub(crate) units: Vec<Unit>,
}

struct Walker<'a> {
    root: PathBuf,
    manifest: &'a Manifest,
    active: BTreeSet<PathBuf>,
    seen: BTreeMap<PathBuf, String>,
    units: BTreeMap<String, Unit>,
}

pub(crate) fn load(root: &Path, manifest: &Manifest) -> Result<Graph, Fault> {
    let base = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut files = BTreeMap::new();
    discover(&base, "", manifest, &mut files)?;
    if files.is_empty() {
        return Err(Fault::Surface(String::from(
            "lemma contains no .sx modules",
        )));
    }
    let mut walker = Walker {
        root: base,
        manifest,
        active: BTreeSet::new(),
        seen: BTreeMap::new(),
        units: BTreeMap::new(),
    };
    for path in files.values() {
        walker.visit(path)?;
    }
    Ok(Graph {
        units: walker.units.into_values().collect(),
    })
}

fn discover(
    current: &Path,
    prefix: &str,
    manifest: &Manifest,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<(), Fault> {
    let entries =
        fs::read_dir(current).map_err(|err| Fault::Io(format!("{}: {err}", current.display())))?;
    for item in entries {
        let entry = item.map_err(|err| Fault::Io(err.to_string()))?;
        let Ok(name) = entry.file_name().into_string() else {
            return Err(Fault::Surface(String::from("non-utf8 path in lemma")));
        };
        if name.starts_with('.') || name == "target" {
            continue;
        }
        let full = entry.path();
        let kind = entry
            .file_type()
            .map_err(|err| Fault::Io(format!("{}: {err}", full.display())))?;
        if kind.is_dir() {
            if full.join("simplex.toml").symlink_metadata().is_ok() {
                continue;
            }
            if prefix.is_empty() && manifest.dependencies.contains_key(&name) {
                return Err(Fault::Surface(format!(
                    "local module path collides with dependency: {name}"
                )));
            }
            discover(&full, &join(prefix, &name), manifest, files)?;
        } else if kind.is_file() && name.ends_with(".sx") {
            let stem = name.strip_suffix(".sx").unwrap_or(&name);
            if prefix.is_empty() && manifest.dependencies.contains_key(stem) {
                return Err(Fault::Surface(format!(
                    "local module path collides with dependency: {stem}"
                )));
            }
            files.insert(join(prefix, &name), full);
        }
    }
    Ok(())
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    }
}

impl<'a> Walker<'a> {
    fn visit(&mut self, target: &Path) -> Result<String, Fault> {
        let key = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
        if let Some(found) = self.seen.get(&key) {
            return Ok(found.clone());
        }
        if !self.active.insert(key.clone()) {
            return Err(Fault::Surface(String::from("cyclic module import")));
        }
        let id = identify(&self.root, &key);
        let program = parse(&key)?;
        let fallback = self.root.clone();
        let dir = key.parent().unwrap_or(&fallback);
        let (locals, external) = self.dependencies(dir, &program)?;
        self.active.remove(&key);
        self.seen.insert(key, id.clone());
        self.units.insert(
            id.clone(),
            Unit {
                id: id.clone(),
                program,
                locals,
                external,
            },
        );
        Ok(id)
    }

    fn dependencies(&mut self, dir: &Path, program: &Program) -> Result<(Links, Links), Fault> {
        let mut locals = BTreeMap::new();
        let mut external = BTreeMap::new();
        for item in &program.items {
            let Statement::Import(imp) = &item.statement else {
                continue;
            };
            let Some(head) = imp.path.first() else {
                return Err(Fault::Surface(String::from("empty import path")));
            };
            if self.manifest.dependencies.contains_key(head) {
                if imp.path.len() == 1 {
                    return Err(Fault::Surface(format!(
                        "external dependency import requires module path: {head}"
                    )));
                }
                external.insert(imp.name.clone(), imp.path.join("."));
                continue;
            }
            let path = self.locate(dir, imp)?;
            let target = self.visit(&path)?;
            locals.insert(imp.name.clone(), target);
        }
        Ok((locals, external))
    }

    fn locate(&self, dir: &Path, imp: &Import) -> Result<PathBuf, Fault> {
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
        Err(Fault::Surface(format!(
            "unresolved local import: {}",
            imp.path.join(".")
        )))
    }
}

fn parse(path: &Path) -> Result<Program, Fault> {
    let source =
        fs::read_to_string(path).map_err(|err| Fault::Io(format!("{}: {err}", path.display())))?;
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Fault::Surface(format!("invalid module filename: {}", path.display())))?;
    simplex::inspect(Some(file), path.parent(), &source)
        .map_err(|finding| Fault::Surface(finding.to_string()))
}

fn identify(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path).with_extension("");
    let parts: Vec<String> = rel
        .components()
        .map(|comp| comp.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join(".")
}
