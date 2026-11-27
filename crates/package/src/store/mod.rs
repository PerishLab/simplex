mod cache;
mod clock;
mod remote;

use crate::Fault;
use crate::archive;
use crate::model::{Coordinate, Version};
use crate::seal::Seal;
use crate::signing::Endpoint;
use remote::Client;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cached {
    pub coordinate: Coordinate,
    pub root: PathBuf,
    pub archive: Vec<u8>,
    pub seal: Seal,
}

#[derive(Clone, Debug)]
pub struct Store {
    cache: PathBuf,
    endpoint: Option<Endpoint>,
}

impl Store {
    pub fn new(cache: PathBuf, endpoint: Option<Endpoint>) -> Self {
        Self { cache, endpoint }
    }

    pub fn publish(&self, archive: &[u8], seal: &Seal) -> Result<(), Fault> {
        let actual = archive::digest(archive);
        if actual != seal.source {
            return Err(Fault::Integrity(format!(
                "archive digest mismatch: expected {}, got {actual}",
                seal.source
            )));
        }
        let Some(endpoint) = &self.endpoint else {
            return Err(Fault::Offline("missing S3 endpoint for publish".into()));
        };
        let client = Client::new(endpoint);
        let prefix = seal.coordinate.prefix();
        let tarball = format!("{prefix}/source.tar");
        let metadata = format!("{prefix}/seal.toml");
        let rendered = seal.render();
        client.put(&tarball, archive)?;
        client.put(&metadata, rendered.as_bytes())?;
        cache::save(&self.cache, archive, seal)?;
        Ok(())
    }

    pub fn fetch(&self, coordinate: &Coordinate) -> Result<Cached, Fault> {
        if let Some(hit) = cache::load(&self.cache, coordinate)? {
            return Ok(hit);
        }
        let Some(endpoint) = &self.endpoint else {
            return Err(Fault::Offline(format!(
                "offline cache miss for {coordinate}"
            )));
        };
        let client = Client::new(endpoint);
        let prefix = coordinate.prefix();
        let raw = client.get(&format!("{prefix}/seal.toml"))?;
        let text = String::from_utf8(raw)
            .map_err(|err| Fault::Integrity(format!("invalid remote seal utf8: {err}")))?;
        let seal = Seal::parse(&text)?;
        if seal.coordinate != *coordinate {
            return Err(Fault::Integrity(format!(
                "remote seal coordinate mismatch: expected {coordinate}, got {}",
                seal.coordinate
            )));
        }
        let bytes = client.get(&format!("{prefix}/source.tar"))?;
        let actual = archive::digest(&bytes);
        if actual != seal.source {
            return Err(Fault::Integrity(format!(
                "remote archive digest mismatch: expected {}, got {actual}",
                seal.source
            )));
        }
        let root = cache::save(&self.cache, &bytes, &seal)?;
        Ok(Cached {
            coordinate: coordinate.clone(),
            root,
            archive: bytes,
            seal,
        })
    }

    pub fn latest(&self, scope: &str, name: &str, channel: &str) -> Result<Option<Version>, Fault> {
        let probe = Version {
            major: 0,
            minor: 0,
            patch: 0,
        };
        Coordinate::new(scope, name, probe, channel)?;
        let Some(endpoint) = &self.endpoint else {
            return Err(Fault::Offline(
                "missing S3 endpoint for latest lookup".into(),
            ));
        };
        let client = Client::new(endpoint);
        let prefix = format!("{scope}/{name}/");
        let keys = client.list(&prefix)?;
        let mut maximum: Option<Version> = None;
        for key in keys {
            if let Some(version) = matching(&key, scope, name, channel) {
                maximum = Some(maximum.map_or(version, |prev| prev.max(version)));
            }
        }
        Ok(maximum)
    }
}

fn matching(key: &str, scope: &str, name: &str, channel: &str) -> Option<Version> {
    let trimmed = key.trim_start_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() != 5 {
        return None;
    }
    if parts[0] != scope || parts[1] != name {
        return None;
    }
    if parts[3] != channel || parts[4] != "seal.toml" {
        return None;
    }
    Version::parse(parts[2]).ok()
}
