use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub bytes: Vec<u8>,
    pub modified: u64,
    pub owner: u64,
    pub group: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub schema: String,
    pub scope: String,
    pub name: String,
    pub version: String,
    pub channel: String,
    pub source: String,
    pub api: String,
    pub abi: String,
    pub dependencies: Vec<BTreeMap<String, String>>,
}

pub fn unpack(archive: &[u8]) -> Vec<Entry> {
    assert_eq!(archive.len() % 512, 0, "unaligned archive");
    let mut cursor = 0;
    let mut entries = Vec::new();
    while cursor + 512 <= archive.len() {
        let block = &archive[cursor..cursor + 512];
        if block.iter().all(|byte| *byte == 0) {
            break;
        }
        let size = octal(&block[124..136]) as usize;
        let owner = octal(&block[108..116]);
        let group = octal(&block[116..124]);
        let modified = octal(&block[136..148]);
        let kind = block[156];
        assert!(
            kind == 0 || kind == b'0',
            "unexpected tar entry kind {kind}"
        );
        let path = name(block);
        let start = cursor + 512;
        let end = start + size;
        assert!(end <= archive.len(), "truncated tar entry {path}");
        entries.push(Entry {
            path,
            bytes: archive[start..end].to_vec(),
            modified,
            owner,
            group,
        });
        cursor = start + size.div_ceil(512) * 512;
    }
    entries
}

fn name(block: &[u8]) -> String {
    let base = text(&block[0..100]);
    let prefix = text(&block[345..500]);
    if prefix.is_empty() {
        base
    } else {
        format!("{prefix}/{base}")
    }
}

fn text(field: &[u8]) -> String {
    let limit = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8(field[..limit].to_vec()).expect("utf8 header")
}

fn octal(field: &[u8]) -> u64 {
    let cleaned = text(field);
    let trimmed = cleaned.trim_matches(|ch: char| ch.is_ascii_whitespace() || ch == '\0');
    if trimmed.is_empty() {
        return 0;
    }
    u64::from_str_radix(trimmed, 8).expect("octal header")
}

pub fn parse(content: &str) -> Stamp {
    let mut root = BTreeMap::new();
    let mut dependencies: Vec<BTreeMap<String, String>> = Vec::new();
    let mut section = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[dependencies]]" {
            section = true;
            dependencies.push(BTreeMap::new());
            continue;
        }
        let (key, value) = pair(line);
        if section {
            dependencies
                .last_mut()
                .expect("dependency table")
                .insert(key, value);
        } else {
            root.insert(key, value);
        }
    }
    build(root, dependencies)
}

fn pair(line: &str) -> (String, String) {
    let (left, right) = line.split_once('=').expect("toml assignment");
    let key = left.trim().to_owned();
    let trimmed = right.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .expect("quoted string value");
    (key, unquoted.to_owned())
}

fn build(mut root: BTreeMap<String, String>, dependencies: Vec<BTreeMap<String, String>>) -> Stamp {
    let source = take(&mut root, "source");
    let api = take(&mut root, "api");
    let abi = take(&mut root, "abi");
    check(&source);
    check(&api);
    check(&abi);
    for item in &dependencies {
        check(item.get("source").expect("dependency source"));
        check(item.get("api").expect("dependency api"));
        check(item.get("abi").expect("dependency abi"));
    }
    Stamp {
        schema: take(&mut root, "schema"),
        scope: take(&mut root, "scope"),
        name: take(&mut root, "name"),
        version: take(&mut root, "version"),
        channel: take(&mut root, "channel"),
        source,
        api,
        abi,
        dependencies,
    }
}

fn take(map: &mut BTreeMap<String, String>, key: &str) -> String {
    map.remove(key)
        .unwrap_or_else(|| panic!("missing seal field {key}"))
}

fn check(digest: &str) {
    assert_eq!(digest.len(), 64, "digest length {digest}");
    assert!(
        digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')),
        "non-lowercase hex digest {digest}"
    );
}
