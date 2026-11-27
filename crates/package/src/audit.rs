use crate::{Contract, Fault, Seal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Initial,
    Compatible,
    Breaking,
    Epoch,
}

#[derive(Clone, Copy, Debug)]
pub struct Release<'a> {
    pub seal: &'a Seal,
    pub contract: &'a Contract,
}

pub fn verify(
    previous: Option<Release<'_>>,
    current: Release<'_>,
    epoch: bool,
) -> Result<Verdict, Fault> {
    let Some(prior) = previous else {
        return Ok(Verdict::Initial);
    };
    let prev = &prior.seal.coordinate;
    let next = &current.seal.coordinate;
    if prev.scope != next.scope || prev.name != next.name {
        return Err(Fault::Version(format!(
            "package identity mismatch: {}/{} vs {}/{}",
            prev.scope, prev.name, next.scope, next.name
        )));
    }
    if next.version <= prev.version {
        return Err(Fault::Version(format!(
            "version {} must advance beyond {}",
            next.version, prev.version
        )));
    }
    if next.version.major > prev.version.major {
        if !epoch {
            return Err(Fault::Epoch(format!(
                "major epoch jump {} -> {} requires --epoch",
                prev.version, next.version
            )));
        }
        return Ok(Verdict::Epoch);
    }
    if let Some(key) = broken(prior.contract, current.contract) {
        if next.version.minor > prev.version.minor {
            return Ok(Verdict::Breaking);
        }
        return Err(Fault::Breaking(format!(
            "breaking contract change in `{key}` requires minor bump ({} -> {})",
            prev.version, next.version
        )));
    }
    Ok(Verdict::Compatible)
}

fn broken(prior: &Contract, next: &Contract) -> Option<String> {
    for (key, expected) in &prior.exports {
        if next.exports.get(key) != Some(expected) {
            return Some(key.clone());
        }
    }
    for (key, expected) in &prior.layouts {
        if next.layouts.get(key) != Some(expected) {
            return Some(key.clone());
        }
    }
    None
}
