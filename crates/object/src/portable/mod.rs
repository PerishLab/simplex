mod format;
mod image;
mod layout;
mod survey;
mod table;

use crate::{Artifact, Fault};

pub fn emit(artifact: &Artifact) -> Result<Vec<u8>, Fault> {
    image::Writer::new(artifact)?.emit()
}
