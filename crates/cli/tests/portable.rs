use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Section {
    virtualized: u32,
    span: u32,
    raw: u32,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn short(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.0[offset], self.0[offset + 1]])
    }

    fn word(&self, offset: usize) -> u32 {
        let slice = &self.0[offset..offset + 4];
        u32::from_le_bytes(slice.try_into().expect("word"))
    }

    fn section(&self, slot: usize) -> Section {
        let span = self.word(slot + 8).max(self.word(slot + 16));
        Section {
            virtualized: self.word(slot + 12),
            span,
            raw: self.word(slot + 20),
        }
    }

    fn entries(&self, start: usize, count: usize) -> usize {
        (0..count)
            .filter(|index| (self.short(start + index * 2) >> 12) == 10)
            .count()
    }
}

struct Image {
    bytes: Vec<u8>,
    optional: usize,
    flags: u16,
    sections: Vec<Section>,
}

impl Image {
    fn open(path: &Path) -> Self {
        let bytes = fs::read(path).expect("read binary");
        assert!(bytes.len() >= 64);
        assert_eq!(&bytes[0..2], b"MZ");
        let view = Reader(&bytes);
        let header = view.word(0x3c) as usize;
        assert_eq!(&bytes[header..header + 4], b"PE\0\0");
        assert_eq!(view.short(header + 4), 0x8664);
        let count = view.short(header + 6) as usize;
        assert!(count >= 1);
        let size = view.short(header + 20) as usize;
        assert!(size >= 240);
        let flags = view.short(header + 22);
        assert_ne!(flags & 0x0002, 0);
        let optional = header + 24;
        assert_eq!(view.short(optional), 0x020b);
        assert!(view.word(optional + 108) >= 6);
        let table = optional + size;
        let sections = (0..count)
            .map(|index| view.section(table + index * 40))
            .collect();
        Self {
            bytes,
            optional,
            flags,
            sections,
        }
    }

    fn verify(&self, shared: bool) {
        assert_eq!((self.flags & 0x2000) != 0, shared);
    }

    fn directory(&self, index: usize) -> (u32, u32) {
        let view = Reader(&self.bytes);
        let slot = self.optional + 112 + index * 8;
        (view.word(slot), view.word(slot + 4))
    }

    fn locate(&self, address: u32) -> usize {
        for item in &self.sections {
            if address >= item.virtualized && address - item.virtualized < item.span {
                return (item.raw + (address - item.virtualized)) as usize;
            }
        }
        panic!("unmapped address {address:#x}");
    }

    fn text(&self, address: u32) -> String {
        let start = self.locate(address);
        let tail = &self.bytes[start..];
        let end = tail.iter().position(|byte| *byte == 0).expect("string");
        String::from_utf8(tail[..end].to_vec()).expect("ascii")
    }

    fn exports(&self) -> Vec<String> {
        let (address, size) = self.directory(0);
        assert!(address != 0 && size >= 40);
        let view = Reader(&self.bytes);
        let base = self.locate(address);
        assert!(view.word(base + 20) > 0);
        let count = view.word(base + 24) as usize;
        assert!(count > 0);
        let names = self.locate(view.word(base + 32));
        (0..count)
            .map(|index| self.text(view.word(names + index * 4)))
            .collect()
    }

    fn imports(&self) -> Vec<String> {
        let (address, size) = self.directory(1);
        assert!(address != 0 && size >= 20);
        let view = Reader(&self.bytes);
        let mut cursor = self.locate(address);
        let mut items = Vec::new();
        while cursor + 20 <= self.bytes.len() {
            let name = view.word(cursor + 12);
            if name == 0 {
                break;
            }
            items.push(self.text(name).to_ascii_lowercase());
            cursor += 20;
        }
        items
    }

    fn relocations(&self) -> usize {
        let (address, size) = self.directory(5);
        if address == 0 || size == 0 {
            return 0;
        }
        let view = Reader(&self.bytes);
        let base = self.locate(address);
        let limit = base + size as usize;
        let mut cursor = base;
        let mut total = 0;
        while cursor < limit {
            let block = view.word(cursor + 4) as usize;
            assert!(block >= 8);
            total += view.entries(cursor + 8, (block - 8) / 2);
            cursor += block;
        }
        total
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
        command.args(["build", "--target", "windows-x86_64"]);
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
    let binary = Image::open(&sandbox.join("zero.exe"));
    binary.verify(false);
    assert!(binary.imports().iter().any(|item| item == "kernel32.dll"));
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
    let path = sandbox.join("external.exe");
    sandbox.compile(&entry, Some(&path), false);
    let image = Image::open(&path);
    image.verify(false);
    let libraries = image.imports();
    assert!(libraries.iter().any(|item| item == "kernel32.dll"));
    assert!(libraries.iter().any(|item| item == "ucrtbase.dll"));
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
    let library = Image::open(&sandbox.join("codec.dll"));
    library.verify(true);
    let symbols = library.exports();
    assert!(symbols.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(symbols.iter().any(|item| item.ends_with("$codec$digest")));
    assert!(symbols.iter().any(|item| item.contains("$transcode")));

    let runner = sandbox.join("runner.exe");
    sandbox.compile(&entry, Some(&runner), false);
    let host = Image::open(&runner);
    host.verify(false);
    let libraries = host.imports();
    assert!(libraries.iter().any(|item| item == "codec.dll"));
    assert!(libraries.iter().any(|item| item == "kernel32.dll"));
    assert!(host.relocations() > 0);
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
    let path = sandbox.join("fiber.exe");
    sandbox.compile(&entry, Some(&path), false);
    let image = Image::open(&path);
    image.verify(false);
    let libraries = image.imports();
    assert!(libraries.iter().any(|item| item == "kernel32.dll"));
    assert!(libraries.iter().any(|item| item == "ucrtbase.dll"));
}
