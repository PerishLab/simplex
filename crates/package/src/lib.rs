pub mod archive;
pub mod audit;
pub mod manifest;
pub mod model;
pub mod seal;
pub mod signing;
pub mod store;
pub mod surface;

pub use audit::{Release, Verdict};
pub use manifest::{Dependency, LEMMA, Manifest, WORKSPACE, Workspace};
pub use model::{Coordinate, Version};
pub use seal::{Locked, SEAL, Seal};
pub use signing::{Credentials, Endpoint};
pub use store::{Cached, Store};
pub use surface::Contract;

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Io(String),
    Coordinate(String),
    Manifest(String),
    Workspace(String),
    Archive(String),
    Integrity(String),
    Surface(String),
    Seal(String),
    Epoch(String),
    Breaking(String),
    Version(String),
    Offline(String),
    Missing(String),
    Immutable(String),
    Remote(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(detail)
            | Self::Coordinate(detail)
            | Self::Manifest(detail)
            | Self::Workspace(detail)
            | Self::Archive(detail)
            | Self::Integrity(detail)
            | Self::Surface(detail)
            | Self::Seal(detail)
            | Self::Epoch(detail)
            | Self::Breaking(detail)
            | Self::Version(detail)
            | Self::Offline(detail)
            | Self::Missing(detail)
            | Self::Immutable(detail)
            | Self::Remote(detail) => out.write_str(detail),
        }
    }
}

impl std::error::Error for Fault {}
