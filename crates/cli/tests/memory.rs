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

    fn interpreter(&self) -> Option<String> {
        let table = self.wide(32) as usize;
        let stride = self.short(54) as usize;
        let count = self.short(56) as usize;
        for index in 0..count {
            let base = table + index * stride;
            if self.word(base) == 3 {
                let start = self.wide(base + 8) as usize;
                let size = self.wide(base + 32) as usize;
                let slice = &self.0[start..start + size];
                let trimmed = slice.strip_suffix(&[0]).unwrap_or(slice);
                return Some(String::from_utf8_lossy(trimmed).into_owned());
            }
        }
        None
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

    fn join(&self, relative: &str) -> PathBuf {
        let root = self.handle.path();
        root.join(relative)
    }

    fn asset(&self, relative: &str, bytes: &[u8]) {
        let target = self.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(&target, bytes).expect("write asset");
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

    fn compile(&self, entry: &Path, output: &Path) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_simplex"));
        command.current_dir(self.handle.path());
        command.arg("build");
        command.arg(entry);
        command.arg("-o");
        command.arg(output);
        let result = command.output().expect("spawn simplex");
        let detail = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "build failed: {detail}");
    }
}

fn execute(binary: &Path) -> Output {
    Command::new(binary).output().expect("run child")
}

#[test]
fn aggregates() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "aggregates.sx",
        "\
Window := struct {
    offset: i64
    length: i64
}
Window : Copy {}
Ledger := struct {
    alpha: i64
    beta: i64
    gamma: i64
    delta: i64
}
Envelope := enum {
    empty
    code(i64)
    full(Ledger)
}
mint := (seed: i64) -> Ledger {
    Ledger { alpha: seed, beta: seed * 2, gamma: seed * 3, delta: seed * 4 }
}
wrap := (active: bool, seed: i64) -> Envelope {
    switch active {
        true => Envelope.full(mint(seed))
        false => Envelope.code(seed)
    }
}
adjust := (target: &var Ledger, step: i64) {
    target.alpha += step
    target.gamma += step * 2
}
unpack := (item: Envelope) switch item {
    Envelope.empty => 0
    Envelope.code(value) => value
    Envelope.full(ledger) => ledger.alpha + ledger.beta + ledger.gamma + ledger.delta
}
pub main := () -> i64 {
    window := Window { offset: 4, length: 9 }
    var direct := mint(5)
    adjust(&var direct, window.offset)
    direct.alpha + direct.gamma + window.length + unpack(wrap(true, 3)) + unpack(wrap(false, 7))
}
",
    );
    let binary = sandbox.join("aggregates");
    sandbox.compile(&entry, &binary);
    assert_eq!(Image::open(&binary).interpreter(), None);
    assert_eq!(execute(&binary).status.code(), Some(78));
}

#[test]
fn slices() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "slices.sx",
        "\
trim := (payload: &[u8], skip: usize, limit: usize) -> &[u8] {
    &payload[skip..limit]
}
paint := (target: &var [u8], base: u8) {
    target[0] = base
    target[1] = base + 3
    target[2] = base + 8
}
fold := (view: &[u8]) -> usize {
    var total: usize = 0
    for index in 0..view.len() { total += view[index].into<usize>() }
    total
}
pub main := () -> i64 {
    var bytes: List<u8> = [10, 20, 30, 40, 50]
    paint(&var bytes[1..4], 7)
    middle := trim(&bytes[0..5], 1, 4)
    (fold(middle) + middle.len() + bytes[0].into<usize>() + bytes[4].into<usize>()).as<i64>()
}
",
    );
    let binary = sandbox.join("slices");
    sandbox.compile(&entry, &binary);
    assert_eq!(execute(&binary).status.code(), Some(95));
}

#[test]
fn assets() {
    let sandbox = Sandbox::open();
    sandbox.asset("assets/weights.bin", &[11, 22, 33, 44, 55]);
    let entry = sandbox.stage(
        "assets.sx",
        "\
weights := embed(\"assets/weights.bin\")
pub main := () -> i64 {
    window := &weights[1..4]
    (weights.len() + weights[0].into<usize>() + window[1].into<usize>() + window.len() + weights[4].into<usize>()).as<i64>()
}
",
    );
    let binary = sandbox.join("weights");
    sandbox.compile(&entry, &binary);
    assert_eq!(execute(&binary).status.code(), Some(107));
}

#[test]
fn cleanup() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "cleanup.sx",
        "\
emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize
Drop := interface {
    drop: (self: &var Self)
}
Guard := struct {
    tag: u8
}
Guard : Drop {
    drop: (self: &var Guard) {
        emit(1, &self.tag, 1)
    }
}
peek := (guard: &Guard) -> i64 {
    (guard.tag - '0').into<i64>()
}
mark := (code: u8) -> i64 {
    var slot: u8 = code
    emit(1, &slot, 1).as<i64>()
}
scoped := (active: bool) -> i64 {
    var first := Guard { tag: '1' }
    var second := Guard { tag: '2' }
    inner := switch active {
        true => {
            var third := Guard { tag: '3' }
            var fourth := Guard { tag: '4' }
            peek(&third) + peek(&fourth)
        }
        false => 0
    }
    var fifth := Guard { tag: '5' }
    mark('!') + inner + (inner - 7) + peek(&first) + peek(&second) + peek(&fifth)
}
pub main := () -> i64 {
    scoped(true)
}
",
    );
    let binary = sandbox.join("cleanup");
    sandbox.compile(&entry, &binary);
    let result = execute(&binary);
    assert_eq!(result.status.code(), Some(16));
    assert_eq!(result.stdout, b"43!521");
}

#[test]
fn external() {
    let sandbox = Sandbox::open();
    sandbox.asset("data/banner.txt", b"simplex-abi\n");
    let entry = sandbox.stage(
        "external.sx",
        "\
banner := embed(\"data/banner.txt\")
emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize
pub main := () -> i64 {
    var tail: List<u8> = [111, 107, 33]
    first := emit(1, &banner[0], banner.len())
    second := emit(1, &tail[0], 3)
    (first + second + (first - 12) + (second - 3)).as<i64>()
}
",
    );
    let binary = sandbox.join("external");
    sandbox.compile(&entry, &binary);
    let image = Image::open(&binary);
    assert_eq!(&image.0[0..4], b"\x7fELF");
    assert!(matches!(image.short(16), 2 | 3));
    assert!(image.segments().contains(&2));
    assert_eq!(
        image.interpreter().as_deref(),
        Some("/lib64/ld-linux-x86-64.so.2")
    );
    let result = execute(&binary);
    assert_eq!(result.status.code(), Some(15));
    assert_eq!(result.stdout, b"simplex-abi\nok!");
}
