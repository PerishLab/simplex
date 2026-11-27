use crate::{Coordinate, Fault};

pub const SEAL: &str = "simplex.seal/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locked {
    pub alias: String,
    pub coordinate: Coordinate,
    pub source: String,
    pub api: String,
    pub abi: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seal {
    pub coordinate: Coordinate,
    pub source: String,
    pub api: String,
    pub abi: String,
    pub dependencies: Vec<Locked>,
}

impl Seal {
    pub fn parse(text: &str) -> Result<Self, Fault> {
        let _ = text;
        Err(Fault::Seal("seal unimplemented in spine".into()))
    }

    pub fn render(&self) -> String {
        format!("schema = \"{SEAL}\"\n")
    }
}
