pub mod layout;
pub mod plan;
pub mod tree;

mod bridge;
mod eval;
mod flow;
mod front;
mod lower;

pub use bridge::bridge;
pub use layout::{Arm, Catalog, Field, Footprint, Layout, Mangle, Passing, Tagged, Type};
pub use plan::{Graph, Linked, Plan, build, register};
pub use tree::{Asset, Case, Expression, Import, Routine, Statement, Table, Unit};

use simplex::Finding;
use std::fmt;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
    Io(String),
    Collision(String),
    Frontend(Finding),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path) => write!(out, "unreadable module: {path}"),
            Self::Collision(detail) => out.write_str(detail),
            Self::Frontend(finding) => write!(out, "{finding}"),
        }
    }
}

impl std::error::Error for Fault {}

pub fn lower(namespace: &str, entry: &Path, shared: bool) -> Result<Unit, Fault> {
    let bundle = front::loader::load(namespace, entry)?;
    lower::engine::compile(namespace, &bundle, shared)
}

pub fn translate(namespace: &str, entry: &Path, shared: bool) -> Result<simplex_ir::Module, Fault> {
    let unit = lower(namespace, entry, shared)?;
    Ok(bridge(&unit))
}
