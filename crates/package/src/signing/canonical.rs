use crate::Fault;

#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub payload: &'a [u8],
}

pub(crate) struct Location {
    pub base: String,
    pub host: String,
    pub prefix: String,
    pub loopback: bool,
}

pub(crate) fn locate(url: &str, bucket: &str) -> Result<Location, Fault> {
    let trimmed = url.trim();
    let (secure, rest) = if let Some(tail) = trimmed.strip_prefix("https://") {
        (true, tail)
    } else if let Some(tail) = trimmed.strip_prefix("http://") {
        (false, tail)
    } else {
        return Err(Fault::Remote(format!("unsupported endpoint url: {url}")));
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if authority.is_empty()
        || authority.contains('@')
        || authority.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(Fault::Remote(format!("invalid endpoint host: {url}")));
    }
    if bucket.trim_matches('/').is_empty() {
        return Err(Fault::Remote("empty bucket name".into()));
    }
    let host = if secure {
        authority.strip_suffix(":443").unwrap_or(authority)
    } else {
        authority.strip_suffix(":80").unwrap_or(authority)
    };
    let scheme = if secure { "https" } else { "http" };
    Ok(Location {
        base: format!("{scheme}://{authority}"),
        host: host.to_ascii_lowercase(),
        prefix: path.trim_matches('/').to_owned(),
        loopback: local(host),
    })
}

pub(crate) fn uri(prefix: &str, bucket: &str, path: &str) -> String {
    let clean = bucket.trim_matches('/');
    let normalized = normalize(clean, path);
    let mut out = String::from("/");
    if !prefix.is_empty() {
        out.push_str(&encode(prefix.as_bytes(), true));
        out.push('/');
    }
    out.push_str(&encode(clean.as_bytes(), true));
    if normalized == "/" {
        out.push('/');
    } else if !normalized.is_empty() {
        out.push('/');
        out.push_str(&encode(normalized.as_bytes(), true));
    }
    out
}

fn normalize<'a>(bucket: &str, path: &'a str) -> &'a str {
    if let Some(after) = path.strip_prefix('/') {
        if after == bucket {
            return "";
        }
        if let Some(rest) = after
            .strip_prefix(bucket)
            .and_then(|tail| tail.strip_prefix('/'))
        {
            return rest;
        }
        return after;
    }
    path
}

pub(crate) fn query(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(String, String)> = raw
        .split('&')
        .filter(|part| !part.is_empty())
        .map(parameter)
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

pub(crate) fn stamp(timestamp: &str) -> Result<&str, Fault> {
    let bytes = timestamp.as_bytes();
    if bytes.len() != 16 || bytes[8] != b'T' || bytes[15] != b'Z' {
        return Err(Fault::Remote(format!(
            "invalid sigv4 timestamp: {timestamp}"
        )));
    }
    if !bytes[..8].iter().all(u8::is_ascii_digit) || !bytes[9..15].iter().all(u8::is_ascii_digit) {
        return Err(Fault::Remote(format!(
            "invalid sigv4 timestamp: {timestamp}"
        )));
    }
    Ok(&timestamp[..8])
}

fn parameter(segment: &str) -> (String, String) {
    let (name, value) = segment.split_once('=').unwrap_or((segment, ""));
    let left = encode(&decode(name), false);
    let right = encode(&decode(value), false);
    (left, right)
}

fn decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        if let Some(decoded) = escaped(bytes, cursor) {
            out.push(decoded);
            cursor += 3;
        } else {
            out.push(bytes[cursor]);
            cursor += 1;
        }
    }
    out
}

fn escaped(bytes: &[u8], cursor: usize) -> Option<u8> {
    if *bytes.get(cursor)? != b'%' {
        return None;
    }
    let upper = (*bytes.get(cursor + 1)? as char).to_digit(16)?;
    let lower = (*bytes.get(cursor + 2)? as char).to_digit(16)?;
    Some(((upper << 4) | lower) as u8)
}

fn encode(bytes: &[u8], slash: bool) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if unreserved || (slash && byte == b'/') {
            out.push(byte as char);
        } else {
            let high = char::from_digit((byte >> 4) as u32, 16)
                .unwrap_or('0')
                .to_ascii_uppercase();
            let low = char::from_digit((byte & 0x0f) as u32, 16)
                .unwrap_or('0')
                .to_ascii_uppercase();
            out.push('%');
            out.push(high);
            out.push(low);
        }
    }
    out
}

fn local(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "[::1]" || lower.starts_with("[::1]:") {
        return true;
    }
    let name = lower.split(':').next().unwrap_or(&lower);
    matches!(name, "localhost" | "127.0.0.1")
}
