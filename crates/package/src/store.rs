use crate::{Coordinate, Endpoint, Fault, Seal, Version};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cached {
    pub coordinate: Coordinate,
    pub root: PathBuf,
    pub archive: Vec<u8>,
    pub seal: Seal,
}

pub struct Store {
    cache: PathBuf,
    endpoint: Option<Endpoint>,
}

impl Store {
    pub fn new(cache: PathBuf, endpoint: Option<Endpoint>) -> Self {
        Self { cache, endpoint }
    }

    pub fn publish(&self, archive: &[u8], seal: &Seal) -> Result<(), Fault> {
        let agent = ureq::Agent::new_with_defaults();
        let _ = (&self.cache, &self.endpoint, agent);
        let _ = (archive, seal);
        Err(Fault::Remote("store unimplemented in spine".into()))
    }

    pub fn fetch(&self, coordinate: &Coordinate) -> Result<Cached, Fault> {
        let _ = (&self.cache, &self.endpoint, coordinate);
        Err(Fault::Offline("store unimplemented in spine".into()))
    }

    pub fn latest(&self, scope: &str, name: &str, channel: &str) -> Result<Option<Version>, Fault> {
        let _ = (&self.cache, &self.endpoint);
        let _ = (scope, name, channel);
        Err(Fault::Offline("store unimplemented in spine".into()))
    }
}
