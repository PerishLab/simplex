const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub label: String,
    pub code: i32,
    pub output: Vec<u8>,
}

pub fn base(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        text.push(symbol((triple >> 18) & 63));
        text.push(symbol((triple >> 12) & 63));
        text.push(pad(chunk.len() >= 2, (triple >> 6) & 63));
        text.push(pad(chunk.len() >= 3, triple & 63));
    }
    text
}

fn symbol(index: u32) -> char {
    char::from(TABLE[index as usize])
}

fn pad(present: bool, index: u32) -> char {
    if present { symbol(index) } else { '=' }
}

pub fn wide(script: &str) -> String {
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    base(&bytes)
}

pub fn parse(raw: &str) -> Vec<Record> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(entry)
        .collect()
}

fn entry(line: &str) -> Record {
    let mut parts = line.splitn(3, ':');
    let label = parts.next().expect("label").to_owned();
    let code = parts
        .next()
        .expect("code")
        .parse::<i32>()
        .expect("exit integer");
    let digits = parts.next().expect("digits");
    Record {
        label,
        code,
        output: decode(digits),
    }
}

fn decode(digits: &str) -> Vec<u8> {
    let raw = digits.as_bytes();
    assert_eq!(raw.len() % 2, 0);
    raw.chunks(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => panic!("invalid hex byte {byte}"),
    }
}
