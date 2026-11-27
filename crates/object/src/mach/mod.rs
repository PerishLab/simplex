mod chain;
mod format;
mod image;
mod layout;
mod sign;
mod survey;
mod trie;

use crate::{Artifact, Fault};

pub fn emit(artifact: &Artifact) -> Result<Vec<u8>, Fault> {
    image::Writer::new(artifact)?.emit()
}
