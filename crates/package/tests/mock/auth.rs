use sha2::{Digest, Sha256};
use simplex_package::Credentials;
use std::collections::BTreeMap;

pub(crate) struct Frame {
    pub method: String,
    pub target: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

pub(crate) struct Config {
    pub bucket: String,
    pub region: String,
    pub credentials: Option<Credentials>,
    pub limit: Option<usize>,
}

pub(crate) fn authorize(config: &Config, frame: &Frame) -> bool {
    let Some(creds) = &config.credentials else {
        return frame.method == "GET" && !frame.headers.contains_key("authorization");
    };
    let Some(timestamp) = frame.headers.get("x-amz-date") else {
        return false;
    };
    let expected = hex(&Sha256::digest(&frame.body));
    if frame.headers.get("x-amz-content-sha256") != Some(&expected) {
        return false;
    }
    let (path, query) = frame.target.split_once('?').unwrap_or((&frame.target, ""));
    let endpoint = simplex_package::Endpoint {
        url: format!(
            "http://{}",
            frame.headers.get("host").cloned().unwrap_or_default()
        ),
        bucket: config.bucket.clone(),
        region: config.region.clone(),
        credentials: Some(creds.clone()),
    };
    let stripped = path
        .strip_prefix(&format!("/{}", config.bucket))
        .unwrap_or(path)
        .trim_start_matches('/');
    let req = simplex_package::signing::Request {
        method: &frame.method,
        path: stripped,
        query,
        payload: &frame.body,
    };
    let pre = frame.headers.get("if-none-match").map(String::as_str);
    let Ok(computed) = simplex_package::signing::sign(&endpoint, req, timestamp, pre) else {
        return false;
    };
    frame.headers.get("authorization") == computed.get("authorization")
}

pub(crate) fn listing(
    objects: &BTreeMap<String, Vec<u8>>,
    limit: Option<usize>,
    query: &str,
) -> (u16, Vec<u8>) {
    let mut params = BTreeMap::new();
    for pair in query.split('&').filter(|item| !item.is_empty()) {
        let (name, val) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(decode(name), decode(val));
    }
    if params.get("list-type").map(String::as_str) != Some("2") {
        return (400, Vec::new());
    }
    let prefix = params.get("prefix").cloned().unwrap_or_default();
    let matched: Vec<&String> = objects
        .keys()
        .filter(|key| key.starts_with(&prefix))
        .collect();
    let start = params
        .get("continuation-token")
        .and_then(|tok| tok.parse::<usize>().ok())
        .unwrap_or(0);
    let page = limit.unwrap_or(1000).max(1);
    let end = (start + page).min(matched.len());
    let truncated = end < matched.len();
    let mut xml = String::from("<ListBucketResult>");
    xml.push_str(&format!("<IsTruncated>{truncated}</IsTruncated>"));
    if truncated {
        xml.push_str(&format!(
            "<NextContinuationToken>{end}</NextContinuationToken>"
        ));
    }
    for key in &matched[start..end] {
        xml.push_str(&format!("<Contents><Key>{key}</Key></Contents>"));
    }
    xml.push_str("</ListBucketResult>");
    (200, xml.into_bytes())
}

fn decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        if let Some(byte) = escaped(bytes, cursor) {
            out.push(byte);
            cursor += 3;
        } else {
            out.push(bytes[cursor]);
            cursor += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn escaped(bytes: &[u8], cursor: usize) -> Option<u8> {
    if *bytes.get(cursor)? != b'%' {
        return None;
    }
    let upper = (*bytes.get(cursor + 1)? as char).to_digit(16)?;
    let lower = (*bytes.get(cursor + 2)? as char).to_digit(16)?;
    Some(((upper << 4) | lower) as u8)
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}
