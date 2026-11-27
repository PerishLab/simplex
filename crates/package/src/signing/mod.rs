mod canonical;
mod digest;

pub use canonical::Request;

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

pub fn sign(
    endpoint: &Endpoint,
    request: Request<'_>,
    timestamp: &str,
    precondition: Option<&str>,
) -> Result<BTreeMap<String, String>, Fault> {
    let location = canonical::locate(&endpoint.url, &endpoint.bucket)?;
    let method = request.method.trim();
    if method.is_empty() {
        return Err(Fault::Remote("empty request method".into()));
    }
    let Some(credentials) = &endpoint.credentials else {
        return unsigned(&location.host, method, precondition);
    };
    authenticated(
        endpoint,
        credentials,
        &location,
        (request, timestamp, precondition),
    )
}

pub(crate) fn target(
    endpoint: &Endpoint,
    path: &str,
    query: &str,
) -> Result<(String, bool), Fault> {
    let location = canonical::locate(&endpoint.url, &endpoint.bucket)?;
    let uri = canonical::uri(&location.prefix, &endpoint.bucket, path);
    let sorted = canonical::query(query);
    let full = if sorted.is_empty() {
        format!("{}{uri}", location.base)
    } else {
        format!("{}{uri}?{sorted}", location.base)
    };
    Ok((full, location.loopback))
}

fn unsigned(
    host: &str,
    method: &str,
    precondition: Option<&str>,
) -> Result<BTreeMap<String, String>, Fault> {
    if !matches!(method, "GET" | "HEAD") {
        return Err(Fault::Remote(format!(
            "credentials required for {method} request"
        )));
    }
    let mut headers = BTreeMap::new();
    headers.insert("host".into(), host.to_owned());
    if let Some(value) = precondition {
        headers.insert("if-none-match".into(), value.trim().to_owned());
    }
    Ok(headers)
}

fn authenticated(
    endpoint: &Endpoint,
    credentials: &Credentials,
    location: &canonical::Location,
    envelope: (Request<'_>, &str, Option<&str>),
) -> Result<BTreeMap<String, String>, Fault> {
    let (request, timestamp, precondition) = envelope;
    let region = endpoint.region.trim();
    if credentials.identity.is_empty() || credentials.secret.is_empty() || region.is_empty() {
        return Err(Fault::Remote("incomplete S3 signing credentials".into()));
    }
    let date = canonical::stamp(timestamp)?;
    let payload = digest::hash(request.payload);
    let mut headers = BTreeMap::new();
    headers.insert("host".into(), location.host.clone());
    if let Some(value) = precondition {
        headers.insert("if-none-match".into(), value.trim().to_owned());
    }
    headers.insert("x-amz-content-sha256".into(), payload.clone());
    headers.insert("x-amz-date".into(), timestamp.to_owned());
    if let Some(token) = credentials
        .token
        .as_deref()
        .map(str::trim)
        .filter(|tok| !tok.is_empty())
    {
        headers.insert("x-amz-security-token".into(), token.to_owned());
    }
    let (lines, signed) = serialize(&headers);
    let path = canonical::uri(&location.prefix, &endpoint.bucket, request.path);
    let query = canonical::query(request.query);
    let method = request.method.trim();
    let canonical = format!("{method}\n{path}\n{query}\n{lines}\n{signed}\n{payload}");
    let scope = format!("{date}/{region}/s3/aws4_request");
    let string = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        digest::hash(canonical.as_bytes())
    );
    let derived = digest::derive(&credentials.secret, date, region);
    let signature = digest::hex(&digest::mac(&derived, string.as_bytes()));
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
        credentials.identity
    );
    headers.insert("authorization".into(), auth);
    Ok(headers)
}

fn serialize(headers: &BTreeMap<String, String>) -> (String, String) {
    let mut lines = String::new();
    let mut names = Vec::with_capacity(headers.len());
    for (name, value) in headers {
        lines.push_str(name);
        lines.push(':');
        lines.push_str(value.trim());
        lines.push('\n');
        names.push(name.as_str());
    }
    (lines, names.join(";"))
}
