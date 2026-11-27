use crate::Fault;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn parse(text: &str) -> Result<Self, Fault> {
        let mut parts = text.split('.');
        let Some(major) = parts.next().and_then(segment) else {
            return Err(Fault::Version(format!("invalid version: {text}")));
        };
        let Some(minor) = parts.next().and_then(segment) else {
            return Err(Fault::Version(format!("invalid version: {text}")));
        };
        let Some(patch) = parts.next().and_then(segment) else {
            return Err(Fault::Version(format!("invalid version: {text}")));
        };
        if parts.next().is_some() {
            return Err(Fault::Version(format!("invalid version: {text}")));
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Coordinate {
    pub scope: String,
    pub name: String,
    pub version: Version,
    pub channel: String,
}

impl Coordinate {
    pub fn new(scope: &str, name: &str, version: Version, channel: &str) -> Result<Self, Fault> {
        if !word(scope) {
            return Err(Fault::Coordinate(format!("invalid scope: {scope}")));
        }
        if !self::name(name) {
            return Err(Fault::Coordinate(format!("invalid name: {name}")));
        }
        if !word(channel) {
            return Err(Fault::Coordinate(format!("invalid channel: {channel}")));
        }
        Ok(Self {
            scope: scope.to_owned(),
            name: name.to_owned(),
            version,
            channel: channel.to_owned(),
        })
    }

    pub fn parse(text: &str) -> Result<Self, Fault> {
        let mut parts = text.split('/');
        let Some(scope) = parts.next() else {
            return Err(Fault::Coordinate(format!("invalid coordinate: {text}")));
        };
        let Some(name) = parts.next() else {
            return Err(Fault::Coordinate(format!("invalid coordinate: {text}")));
        };
        let Some(raw) = parts.next() else {
            return Err(Fault::Coordinate(format!("invalid coordinate: {text}")));
        };
        let Some(channel) = parts.next() else {
            return Err(Fault::Coordinate(format!("invalid coordinate: {text}")));
        };
        if parts.next().is_some() {
            return Err(Fault::Coordinate(format!("invalid coordinate: {text}")));
        };
        let version = Version::parse(raw)?;
        Self::new(scope, name, version, channel)
    }

    pub fn leaf(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or(&self.name)
    }

    pub fn prefix(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.scope, self.name, self.version, self.channel
        )
    }

    pub fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"simplex.dylib/v1\0");
        hasher.update(self.prefix().as_bytes());
        let sum = hasher.finalize();
        let mut out = String::with_capacity(16);
        for byte in &sum[..8] {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    pub fn namespace(&self) -> String {
        self.hash()
    }

    pub fn stem(&self) -> String {
        self.hash()
    }
}

impl fmt::Display for Coordinate {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}", self.prefix())
    }
}

pub fn name(text: &str) -> bool {
    let mut parts = text.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    if !word(first) {
        return false;
    }
    match parts.next() {
        None => true,
        Some(second) => word(second) && parts.next().is_none(),
    }
}

pub(crate) fn word(text: &str) -> bool {
    let Some((&head, tail)) = text.as_bytes().split_first() else {
        return false;
    };
    head.is_ascii_lowercase()
        && tail
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn segment(part: &str) -> Option<u64> {
    if part.is_empty() {
        return None;
    }
    if part.len() > 1 && part.starts_with('0') {
        return None;
    }
    if !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    part.parse::<u64>().ok()
}
