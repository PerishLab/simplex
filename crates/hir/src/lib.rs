pub mod layout;
pub mod tree;

mod eval;
mod flow;
mod front;
mod lower;

pub use layout::{Arm, Catalog, Field, Footprint, Layout, Mangle, Passing, Tagged, Type};
pub use tree::{Asset, Case, Expression, Import, Routine, Statement, Table, Unit};

use simplex::Finding;
use std::fmt;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
    Io(String),
    Frontend(Finding),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path) => write!(out, "unreadable module: {path}"),
            Self::Frontend(finding) => write!(out, "{finding}"),
        }
    }
}

impl std::error::Error for Fault {}

pub fn lower(namespace: &str, entry: &Path, shared: bool) -> Result<Unit, Fault> {
    let bundle = front::loader::load(entry)?;
    lower::engine::compile(namespace, &bundle, shared)
}
