mod workspace;

pub use workspace::Workspace;

use crate::model::word;
use crate::{Coordinate, Fault, Version};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use toml::{Table, Value};

pub const PACKAGE: &str = "simplex.package/v1";
pub const WORKSPACE: &str = "simplex.workspace/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dependency {
    Exact(Coordinate),
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub library: Coordinate,
    pub entry: String,
    pub dependencies: BTreeMap<String, Dependency>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, Fault> {
        let root = text
            .parse::<Table>()
            .map_err(|err| Fault::Manifest(err.to_string()))?;
        for key in root.keys() {
            if !matches!(key.as_str(), "schema" | "library" | "dependencies") {
                return Err(Fault::Manifest(format!("unknown manifest field: {key}")));
            }
        }
        let schema = required(&root, "schema")?;
        if schema != PACKAGE {
            return Err(Fault::Manifest(format!(
                "unsupported manifest schema: {schema}"
            )));
        }
        let Some(Value::Table(table)) = root.get("library") else {
            return Err(Fault::Manifest("missing [library] table".into()));
        };
        let (library, entry) = header(table)?;
        let dependencies = match root.get("dependencies") {
            Some(Value::Table(items)) => links(items)?,
            Some(_) => return Err(Fault::Manifest("invalid [dependencies] table".into())),
            None => BTreeMap::new(),
        };
        Ok(Self {
            library,
            entry,
            dependencies,
        })
    }

    pub fn load(root: &Path) -> Result<Self, Fault> {
        forbid(root)?;
        let path = root.join("simplex.toml");
        let text = fs::read_to_string(&path)
            .map_err(|err| Fault::Io(format!("{}: {err}", path.display())))?;
        Self::parse(&text)
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "schema = \"{PACKAGE}\"");
        let _ = writeln!(out);
        let _ = writeln!(out, "[library]");
        let _ = writeln!(out, "scope = \"{}\"", self.library.scope);
        let _ = writeln!(out, "name = \"{}\"", self.library.name);
        let _ = writeln!(out, "version = \"{}\"", self.library.version);
        let _ = writeln!(out, "channel = \"{}\"", self.library.channel);
        let _ = writeln!(out, "entry = \"{}\"", self.entry);
        if !self.dependencies.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(out, "[dependencies]");
            for (alias, item) in &self.dependencies {
                render(&mut out, alias, item);
            }
        }
        out
    }

    pub fn rewrite(&self, members: &BTreeMap<String, Coordinate>) -> Result<Self, Fault> {
        let mut dependencies = BTreeMap::new();
        for (alias, item) in &self.dependencies {
            dependencies.insert(alias.clone(), resolve(alias, item, members)?);
        }
        Ok(Self {
            library: self.library.clone(),
            entry: self.entry.clone(),
            dependencies,
        })
    }
}

fn resolve(
    alias: &str,
    item: &Dependency,
    members: &BTreeMap<String, Coordinate>,
) -> Result<Dependency, Fault> {
    match item {
        Dependency::Exact(coordinate) => Ok(Dependency::Exact(coordinate.clone())),
        Dependency::Workspace => members
            .get(alias)
            .cloned()
            .map(Dependency::Exact)
            .ok_or_else(|| Fault::Workspace(format!("missing workspace member: {alias}"))),
    }
}

pub(crate) fn forbid(root: &Path) -> Result<(), Fault> {
    if root.join("simplex.lock").symlink_metadata().is_ok() {
        return Err(Fault::Manifest("lockfile forbidden: simplex.lock".into()));
    }
    Ok(())
}

fn header(table: &Table) -> Result<(Coordinate, String), Fault> {
    for key in table.keys() {
        if !matches!(
            key.as_str(),
            "scope" | "name" | "version" | "channel" | "entry"
        ) {
            return Err(Fault::Manifest(format!("unknown library field: {key}")));
        }
    }
    let scope = required(table, "scope")?;
    let name = required(table, "name")?;
    let raw = required(table, "version")?;
    let channel = required(table, "channel")?;
    let version = Version::parse(raw)?;
    let coordinate = Coordinate::new(scope, name, version, channel)?;
    let entry = match table.get("entry") {
        Some(Value::String(value)) => value.clone(),
        Some(_) => return Err(Fault::Manifest("library entry must be a string".into())),
        None => format!("{name}.sx"),
    };
    validate(&entry)?;
    Ok((coordinate, entry))
}

fn validate(entry: &str) -> Result<(), Fault> {
    if !entry.ends_with(".sx") || entry.starts_with('/') || entry.contains("..") {
        return Err(Fault::Manifest(format!("invalid entry path: {entry}")));
    }
    if entry
        .bytes()
        .any(|byte| matches!(byte, b'\\' | b':' | b'"' | 0..=0x1f))
    {
        return Err(Fault::Manifest(format!("invalid entry path: {entry}")));
    }
    for part in entry.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(Fault::Manifest(format!("invalid entry path: {entry}")));
        }
    }
    let leaf = entry.rsplit('/').next().unwrap_or("");
    if leaf.len() <= 3 || matches!(leaf, "mod.sx" | "index.sx" | "lib.sx") {
        return Err(Fault::Manifest(format!("forbidden entry file: {entry}")));
    }
    Ok(())
}

fn links(items: &Table) -> Result<BTreeMap<String, Dependency>, Fault> {
    let mut out = BTreeMap::new();
    for (alias, value) in items {
        if !word(alias) {
            return Err(Fault::Manifest(format!(
                "invalid dependency alias: {alias}"
            )));
        }
        let Value::Table(spec) = value else {
            return Err(Fault::Manifest(format!(
                "dependency {alias} must be an inline table"
            )));
        };
        out.insert(alias.clone(), dependency(alias, spec)?);
    }
    Ok(out)
}

fn dependency(alias: &str, spec: &Table) -> Result<Dependency, Fault> {
    if let Some(marker) = spec.get("workspace") {
        if spec.len() != 1 {
            return Err(Fault::Manifest(format!(
                "dependency {alias} mixes workspace with other fields"
            )));
        }
        if marker.as_str() != Some("*") {
            return Err(Fault::Manifest(format!(
                "dependency {alias} workspace must be \"*\""
            )));
        }
        return Ok(Dependency::Workspace);
    }
    for key in spec.keys() {
        if !matches!(key.as_str(), "scope" | "name" | "version" | "channel") {
            return Err(Fault::Manifest(format!(
                "unknown field in dependency {alias}: {key}"
            )));
        }
    }
    let scope = required(spec, "scope")?;
    let name = required(spec, "name")?;
    let raw = required(spec, "version")?;
    let channel = required(spec, "channel")?;
    let version = Version::parse(raw)?;
    Ok(Dependency::Exact(Coordinate::new(
        scope, name, version, channel,
    )?))
}

fn required<'a>(table: &'a Table, key: &str) -> Result<&'a str, Fault> {
    match table.get(key) {
        Some(Value::String(value)) => Ok(value.as_str()),
        Some(_) => Err(Fault::Manifest(format!("field {key} must be a string"))),
        None => Err(Fault::Manifest(format!("missing required field: {key}"))),
    }
}

fn render(out: &mut String, alias: &str, item: &Dependency) {
    match item {
        Dependency::Exact(coord) => {
            let _ = writeln!(
                out,
                "{alias} = {{ scope = \"{}\", name = \"{}\", version = \"{}\", channel = \"{}\" }}",
                coord.scope, coord.name, coord.version, coord.channel
            );
        }
        Dependency::Workspace => {
            let _ = writeln!(out, "{alias} = {{ workspace = \"*\" }}");
        }
    }
}
