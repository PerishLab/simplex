use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Sandbox {
    handle: tempfile::TempDir,
}

impl Sandbox {
    fn open() -> Self {
        Self {
            handle: tempfile::tempdir().expect("tempdir"),
        }
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
fn sequential() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "sequential.sx",
        "\
Transport := interface {
    send: async (self: &var Self, amount: usize) -> usize
    sync: async (self: &var Self) -> usize
}
Channel := struct {
    queued: usize
}
Channel : Transport {
    send: async (self: &var Channel, amount: usize) -> usize {
        self.queued += amount
        self.queued
    }
    sync: async (self: &var Channel) -> usize {
        self.queued
    }
}
Packet := struct {
    code: usize
    size: usize
}
meta := async (code: usize) -> usize {
    code * 3
}
body := async (size: usize) -> usize {
    size + 11
}
assemble := async (code: usize, size: usize) -> Packet {
    first := meta(code)
    second := body(size)
    Packet { code: first.await, size: second.await }
}
publish := async <T: Transport>(link: &var T, amount: usize) -> usize {
    link.send(amount).await + link.sync().await
}
pub main := async () -> i64 {
    var channel := Channel { queued: 4 }
    packet := assemble(7, 19).await
    sent := publish(&var channel, packet.code).await
    (sent + (sent - 50) + packet.size).as<i64>()
}
",
    );
    let binary = sandbox.join("sequential");
    sandbox.compile(&entry, &binary);
    assert_eq!(execute(&binary).status.code(), Some(80));
}

#[test]
fn gather() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "gather.sx",
        "\
emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize
mark := (code: u8) {
    var slot: u8 = code
    emit(1, &slot, 1)
}
step := async (first: u8, second: u8, value: u64) -> u64 {
    mark(first)
    yield()
    mark(second)
    value
}
scale := async (seed: u64) -> u64 {
    yield()
    seed * 5
}
pub main := async () -> i64 {
    pair := [step('A', 'C', 11), step('B', 'D', 22)].all().await
    seeds: List<u64> = [2, 4, 6]
    mapped := seeds.map((item) { scale(item) }).all().await
    (pair[0] + pair[1] + mapped[0] + mapped[1] + mapped[2] + (seeds.len() - 3).into<u64>()).as<i64>()
}
",
    );
    let binary = sandbox.join("gather");
    sandbox.compile(&entry, &binary);
    let result = execute(&binary);
    assert_eq!(result.status.code(), Some(93));
    assert_eq!(result.stdout, b"ABCD");
}

#[test]
fn cancellation() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "cancellation.sx",
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
mark := (code: u8) {
    var slot: u8 = code
    emit(1, &slot, 1)
}
peek := (guard: &Guard) -> u64 {
    (guard.tag - '0').into<u64>()
}
slow := async (outer: u8, inner: u8, start: u8) -> u64 {
    var first := Guard { tag: outer }
    var second := Guard { tag: inner }
    mark(start)
    yield()
    yield()
    mark('X')
    peek(&first) + peek(&second)
}
fast := async (banner: u8, winner: u64) -> u64 {
    mark(banner)
    winner
}
pub main := async () -> i64 {
    winner := [slow('1', '2', 'a'), slow('3', '4', 'b'), fast('W', 77)].race().await
    (winner + (winner - 77)).as<i64>()
}
",
    );
    let binary = sandbox.join("cancellation");
    sandbox.compile(&entry, &binary);
    let result = execute(&binary);
    assert_eq!(result.status.code(), Some(77));
    assert_eq!(result.stdout, b"abW4321");
}

#[test]
fn recycling() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "recycling.sx",
        "\
Step := enum {
    done
    more
}
pace := (index: u64) bucket {
    index >= 32 => Step.done
    _ => Step.more
}
slow := async (value: u64) -> u64 {
    yield()
    yield()
    value + 100
}
fast := async (value: u64) -> u64 {
    value
}
round := async (index: u64) -> u64 {
    won := [slow(index), fast(index + 1)].race().await
    won + (won - index - 1)
}
pub main := async () -> i64 {
    var index: u64 = 0
    var total: u64 = 0
    loop switch pace(index) {
        Step.done => break (total % 200).as<i64>()
        Step.more => {
            total += round(index).await
            index += 1
        }
    }
}
",
    );
    let binary = sandbox.join("recycling");
    sandbox.compile(&entry, &binary);
    assert_eq!(execute(&binary).status.code(), Some(128));
}
