use crate::Fault;
use std::fmt;

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
        if !word(name) {
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

    pub fn prefix(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.scope, self.name, self.version, self.channel
        )
    }

    pub fn namespace(&self) -> String {
        format!(
            "{}${}${}_{}_{}${}",
            self.scope,
            self.name,
            self.version.major,
            self.version.minor,
            self.version.patch,
            self.channel
        )
    }

    pub fn stem(&self) -> String {
        format!(
            "sx_{}_{}_{}_{}_{}_{}",
            self.scope,
            self.name,
            self.version.major,
            self.version.minor,
            self.version.patch,
            self.channel
        )
    }
}

impl fmt::Display for Coordinate {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}", self.prefix())
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
