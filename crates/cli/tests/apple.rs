use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Entry {
    kind: u32,
    offset: usize,
    size: usize,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn word(&self, offset: usize) -> u32 {
        let slice = &self.0[offset..offset + 4];
        u32::from_le_bytes(slice.try_into().expect("word"))
    }

    fn wide(&self, offset: usize) -> u64 {
        let slice = &self.0[offset..offset + 8];
        u64::from_le_bytes(slice.try_into().expect("wide"))
    }

    fn network(&self, offset: usize) -> u32 {
        let slice = &self.0[offset..offset + 4];
        u32::from_be_bytes(slice.try_into().expect("network"))
    }

    fn walk(&self, count: usize, span: usize) -> Vec<Entry> {
        let mut cursor = 32;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = self.word(cursor);
            let size = self.word(cursor + 4) as usize;
            assert!(size >= 8 && cursor + size <= 32 + span);
            items.push(Entry {
                kind,
                offset: cursor,
                size,
            });
            cursor += size;
        }
        assert_eq!(cursor, 32 + span);
        items
    }

    fn segment(&self, offset: usize, size: usize) {
        assert!(size >= 72);
        let name = &self.0[offset + 8..offset + 18];
        let address = self.wide(offset + 24);
        let span = self.wide(offset + 32);
        let file = self.wide(offset + 40);
        assert_eq!(address % 0x4000, 0);
        assert_eq!(file % 0x4000, 0);
        if name != b"__LINKEDIT" {
            assert_eq!(span % 0x4000, 0);
        }
    }
}

struct Image {
    bytes: Vec<u8>,
    commands: Vec<Entry>,
}

impl Image {
    fn open(path: &Path, shared: bool) -> Self {
        let bytes = fs::read(path).expect("read binary");
        assert!(bytes.len() >= 32);
        let view = Reader(&bytes);
        assert_eq!(view.word(0), 0xfeed_facf);
        assert_eq!(view.word(4), 0x0100_000c);
        let expected = if shared { 6 } else { 2 };
        assert_eq!(view.word(12), expected);
        let count = view.word(16) as usize;
        let span = view.word(20) as usize;
        assert!(count >= 4 && 32 + span <= bytes.len());
        let commands = view.walk(count, span);
        let image = Self { bytes, commands };
        image.verify(shared);
        image
    }

    fn verify(&self, shared: bool) {
        self.segments(shared);
        let identifier = self.find(0x1b).expect("uuid command");
        assert_eq!(identifier.size, 24);
        let fixups = self.find(0x8000_0034).expect("chained fixups");
        self.range(fixups, false);
        let exports = self.find(0x8000_0033).expect("exports trie");
        self.range(exports, shared);
        let trailing = self.commands.last().expect("trailing command");
        assert_eq!(trailing.kind, 0x1d);
        self.signature(trailing);
    }

    fn holds(&self, needle: &[u8]) -> bool {
        self.bytes.windows(needle.len()).any(|part| part == needle)
    }

    fn find(&self, kind: u32) -> Option<&Entry> {
        self.commands.iter().find(|entry| entry.kind == kind)
    }

    fn segments(&self, shared: bool) {
        let view = Reader(&self.bytes);
        let mut count = 0;
        for entry in &self.commands {
            if entry.kind == 0x19 {
                count += 1;
                view.segment(entry.offset, entry.size);
            }
        }
        let minimum = if shared { 2 } else { 3 };
        assert!(count >= minimum);
    }

    fn range(&self, entry: &Entry, nonempty: bool) {
        assert_eq!(entry.size, 16);
        let view = Reader(&self.bytes);
        let start = view.word(entry.offset + 8) as usize;
        let length = view.word(entry.offset + 12) as usize;
        if nonempty {
            assert!(length > 0);
        }
        assert!(start + length <= self.bytes.len());
    }

    fn signature(&self, entry: &Entry) {
        assert_eq!(entry.size, 16);
        let view = Reader(&self.bytes);
        let start = view.word(entry.offset + 8) as usize;
        let length = view.word(entry.offset + 12) as usize;
        assert!(length >= 12 && start + length <= self.bytes.len());
        assert_eq!(view.network(start), 0xfade_0cc0);
        let blob = view.network(start + 4) as usize;
        assert!(blob >= 12 && blob <= length);
        assert!(view.network(start + 8) >= 1);
    }
}

struct Sandbox(tempfile::TempDir);

impl Sandbox {
    fn open() -> Self {
        Self(tempfile::tempdir().expect("tempdir"))
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    fn stage(&self, name: &str, source: &str) -> PathBuf {
        let target = self.join(name);
        fs::write(&target, source).expect("write source");
        if let Err(finding) = simplex::inspect(Some(name), Some(self.0.path()), source) {
            panic!("invalid specimen {name}: {finding}");
        }
        target
    }

    fn compile(&self, entry: &Path, output: Option<&Path>, shared: bool) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_simplex"));
        command.current_dir(self.0.path());
        command.args(["build", "--target", "darwin-aarch64"]);
        command.arg(entry);
        if shared {
            command.arg("--dylib");
        }
        if let Some(target) = output {
            command.arg("-o").arg(target);
        }
        let result = command.output().expect("spawn simplex");
        let detail = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "build failed: {detail}");
    }
}

#[test]
fn standalone() {
    let sandbox = Sandbox::open();
    let minimal = sandbox.stage("zero.sx", "pub main := () -> i64 { 0 }\n");
    sandbox.compile(&minimal, None, false);
    Image::open(&sandbox.join("zero"), false);
}

#[test]
fn memory() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "external.sx",
        "\
emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize
pub main := () -> i64 {
    var bytes: List<u8> = [111, 107]
    emit(1, &bytes[0], 2).as<i64>()
}
",
    );
    let binary = sandbox.join("external");
    sandbox.compile(&entry, Some(&binary), false);
    Image::open(&binary, false);
}

#[test]
fn dynamic() {
    let sandbox = Sandbox::open();
    let unit = sandbox.stage(
        "codec.sx",
        "\
pub dylib Codec := interface {
    encode: (self: &Self, value: u64) -> u64
}
pub dylib digest := (seed: u64) -> u64 { seed + 7 }
pub dylib transcode := (engine: &Codec, value: u64) -> u64 { engine.encode(value) }
",
    );
    let entry = sandbox.stage(
        "main.sx",
        "\
codec := dylib codec
Cipher := struct {
    key: u64
}
Cipher : codec.Codec {
    encode: (self: &Cipher, value: u64) -> u64 { value ^ self.key }
}
pub main := () -> i64 {
    cipher := Cipher { key: 5 }
    (codec.transcode(&cipher, 12) + codec.transcode(&cipher, 6) + codec.digest(3)).as<i64>()
}
",
    );
    sandbox.compile(&unit, None, true);
    let library = Image::open(&sandbox.join("codec.dylib"), true);
    assert!(!sandbox.join("libcodec.dylib").exists());
    assert!(library.holds(b"@executable_path/codec.dylib\0"));

    let runner = sandbox.join("runner");
    sandbox.compile(&entry, Some(&runner), false);
    let host = Image::open(&runner, false);
    assert!(host.holds(b"@executable_path/codec.dylib\0"));
    assert!(!host.holds(b"libcodec.dylib"));
    assert!(host.holds(b"/usr/lib/libSystem.B.dylib"));
}

#[test]
fn fiber() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "fiber.sx",
        "\
slow := async (value: u64) -> u64 {
    yield()
    value + 10
}
fast := async (value: u64) -> u64 {
    value
}
round := async (seed: u64) -> u64 {
    [slow(seed), fast(seed + 1)].race().await
}
pub main := async () -> i64 {
    pair := [slow(2), fast(3)].all().await
    (pair[0] + pair[1] + round(4).await).as<i64>()
}
",
    );
    let binary = sandbox.join("fiber");
    sandbox.compile(&entry, Some(&binary), false);
    Image::open(&binary, false);
}
