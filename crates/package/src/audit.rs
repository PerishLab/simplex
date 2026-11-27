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
    let _ = (previous, current, epoch);
    Err(Fault::Version("audit unimplemented in spine".into()))
}
