use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Image(Vec<u8>);

impl Image {
    fn open(path: &Path) -> Self {
        Self(fs::read(path).expect("read binary"))
    }

    fn short(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.0[offset], self.0[offset + 1]])
    }

    fn word(&self, offset: usize) -> u32 {
        let slice = &self.0[offset..offset + 4];
        u32::from_le_bytes(slice.try_into().expect("word"))
    }

    fn wide(&self, offset: usize) -> u64 {
        let slice = &self.0[offset..offset + 8];
        u64::from_le_bytes(slice.try_into().expect("wide"))
    }

    fn segments(&self) -> Vec<u32> {
        let table = self.wide(32) as usize;
        let stride = self.short(54) as usize;
        let count = self.short(56) as usize;
        (0..count)
            .map(|index| self.word(table + index * stride))
            .collect()
    }
}

struct Sandbox {
    handle: tempfile::TempDir,
}

impl Sandbox {
    fn open() -> Self {
        Self {
            handle: tempfile::tempdir().expect("tempdir"),
        }
    }

    fn root(&self) -> &Path {
        self.handle.path()
    }

    fn join(&self, name: &str) -> PathBuf {
        let root = self.handle.path();
        root.join(name)
    }

    fn stage(&self, name: &str, source: &str) -> PathBuf {
        let target = self.join(name);
        fs::write(&target, source).expect("write source");
        let root = self.handle.path();
        if let Err(finding) = simplex::inspect(Some(name), Some(root), source) {
            panic!("invalid specimen {name}: {finding}");
        }
        target
    }

    fn compile(&self, entry: &Path, output: &Path, shared: bool) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_simplex"));
        command.current_dir(self.handle.path());
        command.arg("build");
        command.arg(entry);
        if shared {
            command.arg("--dylib");
        }
        command.arg("-o");
        command.arg(output);
        let result = command.output().expect("spawn simplex");
        let detail = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "build failed: {detail}");
    }
}

fn execute(binary: &Path, directory: &Path) -> Output {
    let mut command = Command::new(binary);
    command.env("LD_LIBRARY_PATH", directory);
    command.output().expect("run dynamic binary")
}

fn library(bias: u64, salt: u8) -> String {
    format!(
        "\
pub dylib Header := struct {{
    pub magic: u32
    pub scale: u32
}}
pub dylib Codec := interface {{
    encode: (self: &Self, input: &[u8], output: &var [u8]) -> usize
}}
bias := () -> u64 {{
    {bias}
}}
salt := () -> u8 {{
    {salt}
}}
pub dylib digest := (header: &Header) -> u64 {{
    header.magic.into<u64>() * header.scale.into<u64>() + bias()
}}
pub dylib transcode := (engine: &Codec, input: &[u8], output: &var [u8]) -> usize {{
    written := engine.encode(input, &var output[0..2])
    output[0] = output[0] + salt()
    written + (written - written)
}}
"
    )
}

#[test]
fn shared() {
    let sandbox = Sandbox::open();
    let initial = library(10, 3);
    let updated = library(30, 11);
    sandbox.stage("codec.sx", &updated);
    let unit = sandbox.stage("codec.sx", &initial);
    let entry = sandbox.stage(
        "main.sx",
        "\
codec := dylib codec
Cipher := struct {
    key: u8
}
Cipher : codec.Codec {
    encode: (self: &Cipher, input: &[u8], output: &var [u8]) -> usize {
        output[0] = input[0] ^ self.key
        output[1] = input[1] ^ self.key
        2
    }
}
pub main := () -> i64 {
    header := codec.Header { magic: 6, scale: 7 }
    cipher := Cipher { key: 5 }
    var input: List<u8> = [12, 9, 4, 1]
    var output: List<u8> = [0, 0, 0, 0]
    var count: usize = codec.transcode(&cipher, &input[0..2], &var output[0..2])
    count += codec.transcode(&cipher, &input[2..4], &var output[2..4])
    (codec.digest(&header) + header.magic.into<u64>() + count.into<u64>() + output[0].into<u64>() + output[1].into<u64>() + output[2].into<u64>() + output[3].into<u64>()).as<i64>()
}
",
    );

    let artifact = sandbox.join("libcodec.so");
    sandbox.compile(&unit, &artifact, true);
    let image = Image::open(&artifact);
    assert_eq!(&image.0[0..4], b"\x7fELF");
    assert_eq!(image.short(16), 3);
    assert_eq!(image.short(18), 62);
    let kinds = image.segments();
    assert!(kinds.contains(&1));
    assert!(kinds.contains(&2));
    assert!(!kinds.contains(&3));

    let binary = sandbox.join("runner");
    sandbox.compile(&entry, &binary, false);
    let host = Image::open(&binary);
    let linked = host.segments();
    assert!(linked.contains(&2));
    assert!(linked.contains(&3));
    assert_eq!(execute(&binary, sandbox.root()).status.code(), Some(94));

    sandbox.stage("codec.sx", &updated);
    sandbox.compile(&unit, &artifact, true);
    assert_eq!(fs::read(&binary).expect("unchanged binary"), host.0);
    assert_eq!(execute(&binary, sandbox.root()).status.code(), Some(130));
}
