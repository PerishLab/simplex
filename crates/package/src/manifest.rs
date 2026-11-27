use crate::{Coordinate, Fault};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
        let _ = toml::from_str::<toml::Table>(text)
            .map_err(|error| Fault::Manifest(error.to_string()))?;
        Err(Fault::Manifest("manifest unimplemented in spine".into()))
    }

    pub fn load(root: &Path) -> Result<Self, Fault> {
        let _ = root;
        Err(Fault::Manifest("manifest unimplemented in spine".into()))
    }

    pub fn render(&self) -> String {
        #[derive(Serialize)]
        struct Header<'a> {
            schema: &'a str,
        }
        let header = Header { schema: PACKAGE };
        toml::to_string(&header).unwrap_or_default()
    }

    pub fn rewrite(&self, members: &BTreeMap<String, Coordinate>) -> Result<Self, Fault> {
        let _ = members;
        Err(Fault::Workspace("rewrite unimplemented in spine".into()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub root: PathBuf,
    pub members: BTreeMap<String, (PathBuf, Manifest)>,
}

impl Workspace {
    pub fn parse(root: &Path, text: &str) -> Result<Self, Fault> {
        let _ = (root, text);
        Err(Fault::Workspace("workspace unimplemented in spine".into()))
    }

    pub fn load(root: &Path) -> Result<Self, Fault> {
        let _ = root;
        Err(Fault::Workspace("workspace unimplemented in spine".into()))
    }

    pub fn locate(start: &Path) -> Result<Option<Self>, Fault> {
        let _ = start;
        Ok(None)
    }

    pub fn coordinates(&self) -> BTreeMap<String, Coordinate> {
        self.members
            .values()
            .map(|(_, manifest)| (manifest.library.name.clone(), manifest.library.clone()))
            .collect()
    }
}
