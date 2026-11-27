use crate::crypto::{authenticate, derive, digest, render};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;

pub struct Credentials<'a> {
    pub identity: &'a str,
    pub secret: &'a str,
    pub region: &'a str,
    pub token: Option<&'a str>,
}

pub struct Packet {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: BTreeMap<String, String>,
    pub payload: Vec<u8>,
}

pub struct Reply {
    pub status: u16,
    pub reason: &'static str,
    pub kind: &'static str,
    pub body: Vec<u8>,
}

pub fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
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
    String::from_utf8_lossy(&out).into_owned()
}

fn escaped(bytes: &[u8], cursor: usize) -> Option<u8> {
    if bytes.get(cursor).copied()? != b'%' {
        return None;
    }
    let high = nibble(*bytes.get(cursor + 1)?)?;
    let low = nibble(*bytes.get(cursor + 2)?)?;
    Some((high << 4) | low)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn encode(raw: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut text = String::new();
    for byte in raw.bytes() {
        let safe = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if safe {
            text.push(char::from(byte));
        } else {
            text.push('%');
            text.push(char::from(DIGITS[(byte >> 4) as usize]));
            text.push(char::from(DIGITS[(byte & 15) as usize]));
        }
    }
    text
}

pub fn canonicalize(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(String, String)> = raw
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            (encode(&decode(name)), encode(&decode(value)))
        })
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn verify(credentials: &Credentials<'_>, packet: &Packet) -> Result<(), String> {
    let timestamp = packet
        .headers
        .get("x-amz-date")
        .ok_or_else(|| String::from("missing x-amz-date"))?;
    if timestamp.len() != 16 || !timestamp.ends_with('Z') {
        return Err(String::from("malformed x-amz-date"));
    }
    let date = &timestamp[..8];
    let actual = render(&digest(&packet.payload));
    let declared = packet
        .headers
        .get("x-amz-content-sha256")
        .ok_or_else(|| String::from("missing x-amz-content-sha256"))?;
    if declared != &actual {
        return Err(String::from("payload digest mismatch"));
    }
    if let Some(token) = credentials.token
        && packet
            .headers
            .get("x-amz-security-token")
            .map(String::as_str)
            != Some(token)
    {
        return Err(String::from("security token mismatch"));
    }
    let header = packet
        .headers
        .get("authorization")
        .ok_or_else(|| String::from("missing authorization"))?;
    check(credentials, packet, timestamp, (date, &actual, header))
}

fn check(
    credentials: &Credentials<'_>,
    packet: &Packet,
    timestamp: &str,
    parts: (&str, &str, &str),
) -> Result<(), String> {
    let (date, content, header) = parts;
    let rest = header
        .strip_prefix("AWS4-HMAC-SHA256 ")
        .ok_or_else(|| String::from("unsupported scheme"))?;
    let fields = split(rest);
    let scope = format!("{date}/{}/s3/aws4_request", credentials.region);
    let expected = format!("{}/{scope}", credentials.identity);
    if fields.get("Credential").map(String::as_str) != Some(expected.as_str()) {
        return Err(String::from("credential mismatch"));
    }
    let signed = fields
        .get("SignedHeaders")
        .ok_or_else(|| String::from("missing signed headers"))?;
    let formatted = headers(&packet.headers, signed, credentials.token.is_some())?;
    let request = format!(
        "{}\n{}\n{}\n{formatted}\n{signed}\n{content}",
        packet.method,
        packet.path,
        canonicalize(&packet.query)
    );
    let hashed = render(&digest(request.as_bytes()));
    let message = format!("AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{hashed}");
    let signing = derive(credentials.secret, date, credentials.region);
    let computed = render(&authenticate(&signing, message.as_bytes()));
    if fields.get("Signature").map(String::as_str) != Some(computed.as_str()) {
        return Err(String::from("signature mismatch"));
    }
    Ok(())
}

fn split(rest: &str) -> BTreeMap<String, String> {
    rest.split(',')
        .filter_map(|item| item.trim().split_once('='))
        .map(|(left, right)| (left.trim().to_owned(), right.trim().to_owned()))
        .collect()
}

fn headers(
    present: &BTreeMap<String, String>,
    signed: &str,
    tokenized: bool,
) -> Result<String, String> {
    let names: Vec<&str> = signed.split(';').collect();
    if !names.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(String::from("unsorted signed headers"));
    }
    for mandatory in ["host", "x-amz-content-sha256", "x-amz-date"] {
        if !names.contains(&mandatory) {
            return Err(format!("unsigned mandatory header {mandatory}"));
        }
    }
    if tokenized && !names.contains(&"x-amz-security-token") {
        return Err(String::from("unsigned security token"));
    }
    let mut lines = String::new();
    for name in names {
        let value = present
            .get(name)
            .ok_or_else(|| format!("missing signed header {name}"))?;
        let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
        lines.push_str(&format!("{name}:{collapsed}\n"));
    }
    Ok(lines)
}

pub fn receive(stream: &mut TcpStream) -> Option<Packet> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let boundary = loop {
        if let Some(index) = buffer.windows(4).position(|part| part == b"\r\n\r\n") {
            break index;
        }
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..count]);
    };
    let head = std::str::from_utf8(&buffer[..boundary]).ok()?;
    let (method, target, parsed) = metadata(head)?;
    let length = parsed
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let start = boundary + 4;
    while buffer.len() < start + length {
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..count]);
    }
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    Some(Packet {
        method,
        path: path.to_owned(),
        query: query.to_owned(),
        headers: parsed,
        payload: buffer[start..start + length].to_vec(),
    })
}

fn metadata(head: &str) -> Option<(String, String, BTreeMap<String, String>)> {
    let mut lines = head.split("\r\n");
    let first = lines.next()?;
    let mut tokens = first.split_whitespace();
    let method = tokens.next()?.to_owned();
    let target = tokens.next()?.to_owned();
    let mut parsed = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            parsed.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    Some((method, target, parsed))
}

pub fn send(stream: &mut TcpStream, reply: Reply, head: bool) {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.reason,
        reply.kind,
        reply.body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    if !head {
        let _ = stream.write_all(&reply.body);
    }
    let _ = stream.flush();
}
