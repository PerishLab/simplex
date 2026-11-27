mod dynamic;
mod format;
mod image;
mod layout;
mod section;

use crate::{Artifact, Fault};

pub const INTERPRETER: &str = "/lib64/ld-linux-x86-64.so.2";
pub const BASE: u64 = 0x0040_0000;
pub const PAGE: u64 = 0x1000;

pub fn emit(artifact: &Artifact) -> Result<Vec<u8>, Fault> {
    image::Writer::new(artifact)?.emit()
}
