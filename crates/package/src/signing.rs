use crate::Fault;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credentials {
    pub identity: String,
    pub secret: String,
    pub token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub url: String,
    pub bucket: String,
    pub region: String,
    pub credentials: Option<Credentials>,
}

#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub payload: &'a [u8],
}

pub fn sign(
    endpoint: &Endpoint,
    request: Request<'_>,
    timestamp: &str,
    precondition: Option<&str>,
) -> Result<BTreeMap<String, String>, Fault> {
    let _ = (endpoint, request);
    let _ = (timestamp, precondition);
    Err(Fault::Remote("signing unimplemented in spine".into()))
}
