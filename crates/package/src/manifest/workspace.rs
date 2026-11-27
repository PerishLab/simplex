use super::{Manifest, PACKAGE, WORKSPACE, forbid};
use crate::{Coordinate, Fault};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub root: PathBuf,
    pub members: BTreeMap<String, (PathBuf, Manifest)>,
}

impl Workspace {
    pub fn parse(root: &Path, text: &str) -> Result<Self, Fault> {
        forbid(root)?;
        let table = text
            .parse::<Table>()
            .map_err(|err| Fault::Workspace(err.to_string()))?;
        for key in table.keys() {
            if !matches!(key.as_str(), "schema" | "members") {
                return Err(Fault::Workspace(format!("unknown workspace field: {key}")));
            }
        }
        let Some(Value::String(schema)) = table.get("schema") else {
            return Err(Fault::Workspace("missing workspace schema".into()));
        };
        if schema != WORKSPACE {
            return Err(Fault::Workspace(format!(
                "unsupported workspace schema: {schema}"
            )));
        }
        let Some(Value::Array(list)) = table.get("members") else {
            return Err(Fault::Workspace("missing workspace members list".into()));
        };
        let members = load(root, list)?;
        Ok(Self {
            root: root.to_path_buf(),
            members,
        })
    }

    pub fn load(root: &Path) -> Result<Self, Fault> {
        forbid(root)?;
        let path = root.join("simplex.toml");
        let text = fs::read_to_string(&path)
            .map_err(|err| Fault::Io(format!("{}: {err}", path.display())))?;
        Self::parse(root, &text)
    }

    pub fn locate(start: &Path) -> Result<Option<Self>, Fault> {
        let base = if start.is_file() {
            start.parent().unwrap_or(start)
        } else {
            start
        };
        if let Some(found) = walk(base)? {
            return Ok(Some(found));
        }
        if !base.is_absolute()
            && let Ok(resolved) = fs::canonicalize(base)
        {
            return walk(&resolved);
        }
        Ok(None)
    }

    pub fn coordinates(&self) -> BTreeMap<String, Coordinate> {
        self.members
            .iter()
            .map(|(name, (_, manifest))| (name.clone(), manifest.library.clone()))
            .collect()
    }
}

fn load(root: &Path, list: &[Value]) -> Result<BTreeMap<String, (PathBuf, Manifest)>, Fault> {
    let mut seen = BTreeSet::new();
    let mut members = BTreeMap::new();
    for item in list {
        let Value::String(rel) = item else {
            return Err(Fault::Workspace("workspace member must be a string".into()));
        };
        relative(rel)?;
        if !seen.insert(rel.as_str()) {
            return Err(Fault::Workspace(format!(
                "duplicate workspace member path: {rel}"
            )));
        }
        let folder = root.join(rel);
        let manifest = Manifest::load(&folder)?;
        let name = manifest.library.name.clone();
        if members.contains_key(&name) {
            return Err(Fault::Workspace(format!(
                "duplicate workspace member name: {name}"
            )));
        }
        members.insert(name, (folder, manifest));
    }
    Ok(members)
}

fn relative(path: &str) -> Result<(), Fault> {
    if path.is_empty() || path.starts_with('/') || path.contains("..") {
        return Err(Fault::Workspace(format!("invalid member path: {path}")));
    }
    if path.bytes().any(|byte| matches!(byte, b'\\' | b':' | 0)) {
        return Err(Fault::Workspace(format!("invalid member path: {path}")));
    }
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(Fault::Workspace(format!("invalid member path: {path}")));
        }
    }
    Ok(())
}

fn walk(base: &Path) -> Result<Option<Workspace>, Fault> {
    for current in base.ancestors() {
        let folder = if current.as_os_str().is_empty() {
            Path::new(".")
        } else {
            current
        };
        if let Some(workspace) = probe(folder)? {
            return Ok(Some(workspace));
        }
    }
    Ok(None)
}

fn probe(folder: &Path) -> Result<Option<Workspace>, Fault> {
    let manifest = folder.join("simplex.toml");
    if !manifest.is_file() {
        return Ok(None);
    }
    forbid(folder)?;
    let text = fs::read_to_string(&manifest)
        .map_err(|err| Fault::Io(format!("{}: {err}", manifest.display())))?;
    let table = text
        .parse::<Table>()
        .map_err(|err| Fault::Manifest(err.to_string()))?;
    match table.get("schema").and_then(Value::as_str) {
        Some(WORKSPACE) => Workspace::load(folder).map(Some),
        Some(PACKAGE) => Ok(None),
        Some(other) => Err(Fault::Manifest(format!("unsupported schema: {other}"))),
        None => Err(Fault::Manifest("missing schema field".into())),
    }
}
