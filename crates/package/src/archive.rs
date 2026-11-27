use crate::{Fault, Manifest};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn digest(bytes: &[u8]) -> String {
    let sum = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in sum {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

pub fn pack(root: &Path, manifest: &Manifest) -> Result<(Vec<u8>, String), Fault> {
    let builder = tar::Builder::new(Vec::new());
    let bytes = builder
        .into_inner()
        .map_err(|error| Fault::Archive(error.to_string()))?;
    let _ = (root, manifest, bytes);
    Err(Fault::Archive("archive unimplemented in spine".into()))
}

pub fn unpack(bytes: &[u8], expected: &str, destination: &Path) -> Result<(), Fault> {
    if digest(bytes) != expected {
        return Err(Fault::Integrity("archive digest mismatch".into()));
    }
    let _ = destination;
    Err(Fault::Archive("archive unimplemented in spine".into()))
}
