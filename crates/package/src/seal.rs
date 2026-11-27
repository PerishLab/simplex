use crate::{Coordinate, Fault, Version};
use serde::Deserialize;
use std::collections::BTreeSet;

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    scope: String,
    name: String,
    version: String,
    channel: String,
    source: String,
    api: String,
    abi: String,
    #[serde(default)]
    dependencies: Vec<Record>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    alias: String,
    scope: String,
    name: String,
    version: String,
    channel: String,
    source: String,
    api: String,
    abi: String,
}

impl Seal {
    pub fn parse(text: &str) -> Result<Self, Fault> {
        let doc: Document =
            toml::from_str(text).map_err(|err| Fault::Seal(format!("invalid seal toml: {err}")))?;
        if doc.schema != SEAL {
            return Err(Fault::Seal(format!(
                "unsupported seal schema: {}",
                doc.schema
            )));
        }
        let version = Version::parse(&doc.version)?;
        let coordinate = Coordinate::new(&doc.scope, &doc.name, version, &doc.channel)?;
        let source = digest("source", &doc.source)?;
        let api = digest("api", &doc.api)?;
        let abi = digest("abi", &doc.abi)?;
        let mut seen = BTreeSet::new();
        let mut dependencies = Vec::with_capacity(doc.dependencies.len());
        for item in doc.dependencies {
            let locked = convert(item)?;
            if !seen.insert(locked.alias.clone()) {
                return Err(Fault::Seal(format!(
                    "duplicate dependency alias: {}",
                    locked.alias
                )));
            }
            dependencies.push(locked);
        }
        dependencies.sort_by(|left, right| left.alias.cmp(&right.alias));
        Ok(Self {
            coordinate,
            source,
            api,
            abi,
            dependencies,
        })
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        write(&mut out, "schema", SEAL);
        write(&mut out, "scope", &self.coordinate.scope);
        write(&mut out, "name", &self.coordinate.name);
        write(&mut out, "version", &self.coordinate.version.to_string());
        write(&mut out, "channel", &self.coordinate.channel);
        write(&mut out, "source", &self.source);
        write(&mut out, "api", &self.api);
        write(&mut out, "abi", &self.abi);
        let mut sorted = self.dependencies.clone();
        sorted.sort_by(|left, right| {
            left.alias
                .cmp(&right.alias)
                .then_with(|| left.coordinate.cmp(&right.coordinate))
        });
        for item in &sorted {
            out.push_str("\n[[dependencies]]\n");
            write(&mut out, "alias", &item.alias);
            write(&mut out, "scope", &item.coordinate.scope);
            write(&mut out, "name", &item.coordinate.name);
            write(&mut out, "version", &item.coordinate.version.to_string());
            write(&mut out, "channel", &item.coordinate.channel);
            write(&mut out, "source", &item.source);
            write(&mut out, "api", &item.api);
            write(&mut out, "abi", &item.abi);
        }
        out
    }
}

fn convert(item: Record) -> Result<Locked, Fault> {
    let alias = word("alias", &item.alias)?;
    let version = Version::parse(&item.version)?;
    let coordinate = Coordinate::new(&item.scope, &item.name, version, &item.channel)?;
    let source = digest("dependency source", &item.source)?;
    let api = digest("dependency api", &item.api)?;
    let abi = digest("dependency abi", &item.abi)?;
    Ok(Locked {
        alias,
        coordinate,
        source,
        api,
        abi,
    })
}

fn digest(label: &str, value: &str) -> Result<String, Fault> {
    let valid = value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !valid {
        return Err(Fault::Seal(format!(
            "invalid {label} sha256 digest: {value}"
        )));
    }
    Ok(value.to_owned())
}

fn word(label: &str, value: &str) -> Result<String, Fault> {
    let bytes = value.as_bytes();
    let Some((first, tail)) = bytes.split_first() else {
        return Err(Fault::Seal(format!("empty {label}")));
    };
    let ok = first.is_ascii_lowercase()
        && tail
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    if !ok {
        return Err(Fault::Seal(format!("invalid {label}: {value}")));
    }
    Ok(value.to_owned())
}

fn write(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(" = \"");
    for ch in value.chars() {
        if matches!(ch, '\\' | '"') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push_str("\"\n");
}
