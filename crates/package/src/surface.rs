use crate::{Fault, Manifest};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contract {
    pub api: String,
    pub abi: String,
    pub exports: BTreeMap<String, String>,
    pub layouts: BTreeMap<String, String>,
}

pub fn extract(root: &Path, manifest: &Manifest) -> Result<Contract, Fault> {
    let _ = (root, manifest, simplex::name());
    Err(Fault::Surface("surface unimplemented in spine".into()))
}
