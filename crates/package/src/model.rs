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
        let major = segment(parts.next(), text)?;
        let minor = segment(parts.next(), text)?;
        let patch = segment(parts.next(), text)?;
        if parts.next().is_some() {
            return Err(Fault::Coordinate(format!("invalid version: {text}")));
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
        atom(scope, "scope")?;
        atom(name, "name")?;
        atom(channel, "channel")?;
        Ok(Self {
            scope: scope.to_owned(),
            name: name.to_owned(),
            version,
            channel: channel.to_owned(),
        })
    }

    pub fn parse(text: &str) -> Result<Self, Fault> {
        let parts: Vec<&str> = text.split('/').collect();
        if parts.len() != 4 {
            return Err(Fault::Coordinate(format!("invalid coordinate: {text}")));
        }
        let version = Version::parse(parts[2])?;
        Self::new(parts[0], parts[1], version, parts[3])
    }

    pub fn prefix(&self) -> String {
        format!(
            "{}/{}/{}.{}.{}/{}",
            self.scope,
            self.name,
            self.version.major,
            self.version.minor,
            self.version.patch,
            self.channel
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
        out.write_str(&self.prefix())
    }
}

fn segment(part: Option<&str>, full: &str) -> Result<u64, Fault> {
    let Some(raw) = part else {
        return Err(Fault::Coordinate(format!("invalid version: {full}")));
    };
    if raw.is_empty() || (raw.len() > 1 && raw.starts_with('0')) {
        return Err(Fault::Coordinate(format!("invalid version: {full}")));
    }
    if !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Fault::Coordinate(format!("invalid version: {full}")));
    }
    raw.parse::<u64>()
        .map_err(|_| Fault::Coordinate(format!("invalid version: {full}")))
}

fn atom(value: &str, field: &str) -> Result<(), Fault> {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(Fault::Coordinate(format!("empty {field}")));
    };
    if !first.is_ascii_lowercase()
        || !bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(Fault::Coordinate(format!("invalid {field}: {value}")));
    }
    Ok(())
}
