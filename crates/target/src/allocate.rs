use crate::machine::{Bank, Location, Span};
use std::collections::BTreeMap;

pub struct Plan {
    pub locations: BTreeMap<u32, Location>,
    pub used: Vec<u8>,
    pub frame: u32,
}

pub fn assign(bank: &Bank, spans: &[Span]) -> Plan {
    let _ = (bank, spans);
    Plan {
        locations: BTreeMap::new(),
        used: Vec::new(),
        frame: 0,
    }
}
