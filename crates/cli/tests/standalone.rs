use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn check(binary: &Path) {
    let bytes = fs::read(binary).expect("read binary");
    assert!(bytes.len() >= 64);
    assert_eq!(&bytes[0..6], b"\x7fELF\x02\x01");
    assert_eq!(u16::from_le_bytes([bytes[16], bytes[17]]), 2);
    assert_eq!(u16::from_le_bytes([bytes[18], bytes[19]]), 62);
    assert_ne!(
        u64::from_le_bytes(bytes[24..32].try_into().expect("entry")),
        0
    );
    let table = u64::from_le_bytes(bytes[32..40].try_into().expect("table")) as usize;
    let stride = u16::from_le_bytes([bytes[54], bytes[55]]) as usize;
    let count = u16::from_le_bytes([bytes[56], bytes[57]]) as usize;
    let kinds: Vec<u32> = (0..count)
        .map(|index| {
            let slot = table + index * stride;
            u32::from_le_bytes(bytes[slot..slot + 4].try_into().expect("kind"))
        })
        .collect();
    assert!(kinds.contains(&1));
    assert!(!kinds.contains(&2));
    assert!(!kinds.contains(&3));
}

struct Sandbox(tempfile::TempDir);

impl Sandbox {
    fn open() -> Self {
        Self(tempfile::tempdir().expect("tempdir"))
    }

    fn join(&self, relative: &str) -> PathBuf {
        let root = self.0.path();
        root.join(relative)
    }

    fn stage(&self, relative: &str, source: &str) -> PathBuf {
        let target = self.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(&target, source).expect("write source");
        let name = target.file_name().and_then(|item| item.to_str());
        if let Err(finding) = simplex::inspect(name, target.parent(), source) {
            panic!("invalid specimen {relative}: {finding}");
        }
        target
    }

    fn compile(&self, entry: &Path, output: Option<&Path>) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_simplex"));
        command.current_dir(self.0.path());
        command.arg("build");
        command.arg(entry);
        if let Some(target) = output {
            command.arg("-o");
            command.arg(target);
        }
        let result = command.output().expect("spawn simplex");
        let detail = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "build failed: {detail}");
    }

    fn run(&self, name: &str, source: &str) -> Option<i32> {
        let entry = self.stage(name, source);
        let output = self.join("program");
        self.compile(&entry, Some(&output));
        check(&output);
        execute(&output).status.code()
    }
}

fn execute(binary: &Path) -> Output {
    Command::new(binary).output().expect("run child")
}

#[test]
fn arithmetic() {
    let sandbox = Sandbox::open();
    let entry = sandbox.stage(
        "calc.sx",
        "\
step := (base: i64, shift: i64) -> i64 {
    wide := (base * 7 - 3) / 2
    masked := (wide << shift) ^ 15
    (masked & 255) + (masked >> 4) + (wide % 5)
}
pub main := () -> i64 {
    port: u16 = 12
    lifted := port.into<i64>() + (port - port).into<i64>()
    first := step(lifted, 1)
    second := step(first + lifted, 0)
    bonus := switch second > first {
        true => 6
        false => 19
    }
    ((first + second + bonus) % 200) - (bonus & 1)
}
",
    );
    let minimal = sandbox.stage("zero.sx", "pub main := () -> i64 { 0 }\n");
    let binary = sandbox.join("calc.out");
    sandbox.compile(&entry, Some(&binary));
    check(&binary);
    assert_eq!(execute(&binary).status.code(), Some(67));
    sandbox.compile(&minimal, None);
    let implicit = sandbox.join("zero");
    check(&implicit);
    assert_eq!(execute(&implicit).status.code(), Some(0));
}

#[test]
fn control() {
    let sandbox = Sandbox::open();
    let source = "\
Stage := enum {
    base
    step
}
Tier := enum {
    low
    mid
    high
}
Step := enum {
    done
    skip
    keep
}
phase := (count: i64) bucket {
    count <= 1 => Stage.base
    _ => Stage.step
}
series := (count: i64) -> i64 switch phase(count) {
    Stage.base => count
    Stage.step => series(count - 1) + series(count - 2)
}
grade := (score: i64) bucket score {
    0..20 => Tier.low
    20..60 => Tier.mid
    _ => Tier.high
}
weight := (tier: Tier) switch tier {
    Tier.low => 3
    Tier.mid => 7
    Tier.high => 11
}
next := (cursor: i64, limit: i64) bucket {
    cursor >= limit => Step.done
    cursor == 2 => Step.skip
    _ => Step.keep
}
scan := (limit: i64) -> i64 {
    var cursor: i64 = 0
    var total: i64 = 0
    loop switch next(cursor, limit) {
        Step.done => break total
        Step.skip => {
            cursor += 1
            continue
        }
        Step.keep => {
            total += weight(grade(cursor * 15))
            cursor += 1
        }
    }
}
pub main := () -> i64 {
    tenth := series(10)
    var range: i64 = 0
    for item in 1..5 { range += item }
    (tenth + scan(5) + range) - (tenth - 55)
}
";
    assert_eq!(sandbox.run("flow.sx", source), Some(89));
}

#[test]
fn modules() {
    let sandbox = Sandbox::open();
    let leaf = "pub scale := (value: i64) -> i64 { value * 3 }\n";
    sandbox.stage("nested/leaf.sx", leaf);
    sandbox.stage(
        "span.sx",
        "\
pub Span := struct {
    pub offset: i64
    pub length: i64
}
secret := (raw: i64) -> i64 { raw + 4 }
pub measure := (window: &Span) -> i64 { secret(window.offset + window.length) }
",
    );
    sandbox.stage(
        "gate.sx",
        "\
span := import span
pub eval := (start: i64, width: i64) -> i64 {
    window := span.Span { offset: start, length: width }
    span.measure(&window) + window.offset
}
",
    );
    let source = "\
span := import span
gate := import gate
leaf := import nested.leaf
pub main := () -> i64 {
    base := span.Span { offset: 5, length: 12 }
    first := span.measure(&base) + base.length
    second := gate.eval(7, 9)
    (leaf.scale(first) + second) - (second / 9) - first
}
";
    assert_eq!(sandbox.run("main.sx", source), Some(90));
}

#[test]
fn metaprogramming() {
    let sandbox = Sandbox::open();
    let source = "\
Mode := enum {
    idle
    active
    done
}
Equal := interface {
    same: (self: &Self, other: &Self) -> bool
}
@derive(Equal)
Point := struct {
    x: u64
    @secret
    y: u64
}
mask := comptime {
    var total: u64 = 0
    for bit in 0..4 { total = (total << 1) | 1 }
    total
}
comptime {
    assert(mask == 15)
    assert(fields<Point>().len() == 2)
    assert(variants<Mode>().len() == 3)
    assert(size<Point>() == 16)
}
align := (value: u64, comptime stride: u64) -> u64 {
    comptime { assert(stride == 8) }
    (value + stride - 1) / stride * stride
}
sum := <T>(target: &T) -> u64 {
    var total: u64 = 0
    comptime for field in fields<T>() { total += get(target, field).into<u64>() }
    total
}
hidden := () -> u64 {
    var count: u64 = 0
    comptime for field in fields<Point>() {
        count += switch has(field, secret) {
            true => 1
            false => 0
        }
    }
    count
}
@caller
stamp := (seed: u64) -> u64 {
    site := caller()
    seed + site.line + (site.column - site.column)
}
pub main := () -> i64 {
    origin := Point { x: 10, y: 20 }
    twin := Point { x: 10, y: 20 }
    other := Point { x: 10, y: 99 }
    matched := switch origin.same(&twin) && !origin.same(&other) {
        true => sum(&origin)
        false => 0
    }
    first := stamp(0)
    second := stamp(0)
    (matched + (matched / 10) + align(13, 8) + mask + hidden() + (second - first) + (second - first) + (sum(&other) - sum(&twin))).as<i64>()
}
";
    assert_eq!(sandbox.run("meta.sx", source), Some(146));
}
