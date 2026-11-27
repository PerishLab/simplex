mod export;
mod layout;
mod loader;
mod measure;
mod resolve;
mod syntax;
mod types;

use crate::{Fault, Manifest};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contract {
    pub api: String,
    pub abi: String,
    pub exports: BTreeMap<String, String>,
    pub layouts: BTreeMap<String, String>,
}

pub fn extract(root: &Path, manifest: &Manifest) -> Result<Contract, Fault> {
    let graph = loader::load(root, manifest)?;
    let exports = export::collect(&graph);
    let layouts = layout::collect(&graph);
    let api = hash("simplex.api/v1", &exports);
    let abi = hash("simplex.abi/v1", &layouts);
    Ok(Contract {
        api,
        abi,
        exports,
        layouts,
    })
}

fn hash(domain: &str, entries: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_le_bytes());
    hasher.update(domain.as_bytes());
    hasher.update((entries.len() as u64).to_le_bytes());
    for (key, value) in entries {
        hasher.update((key.len() as u64).to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    let raw = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in raw {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
