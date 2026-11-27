use crate::armor::Record;
use crate::runner::Workspace;
use std::path::PathBuf;

pub struct Case {
    pub label: &'static str,
    pub entry: PathBuf,
    pub expected: Record,
}

pub fn record(label: &str, code: i32, output: &[u8]) -> Record {
    Record {
        label: label.to_owned(),
        code,
        output: output.to_vec(),
    }
}

pub fn expected() -> Vec<Record> {
    vec![
        record("arithmetic", 67, b""),
        record("zero", 0, b""),
        record("control", 89, b""),
        record("modules", 90, b""),
        record("metaprogramming", 146, b""),
    ]
}

pub fn stage(workspace: &Workspace) -> Vec<Case> {
    let calc = workspace.stage("calc.sx", arithmetic());
    let zero = workspace.stage("zero.sx", "pub main := () -> i64 { 0 }\n");
    let flow = workspace.stage("flow.sx", control());
    let modular = modules(workspace);
    let meta = workspace.stage("meta.sx", metaprogramming());
    vec![
        Case {
            label: "arithmetic",
            entry: calc,
            expected: record("arithmetic", 67, b""),
        },
        Case {
            label: "zero",
            entry: zero,
            expected: record("zero", 0, b""),
        },
        Case {
            label: "control",
            entry: flow,
            expected: record("control", 89, b""),
        },
        Case {
            label: "modules",
            entry: modular,
            expected: record("modules", 90, b""),
        },
        Case {
            label: "metaprogramming",
            entry: meta,
            expected: record("metaprogramming", 146, b""),
        },
    ]
}

fn modules(workspace: &Workspace) -> PathBuf {
    workspace.stage(
        "nested/leaf.sx",
        "pub scale := (value: i64) -> i64 { value * 3 }\n",
    );
    workspace.stage(
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
    workspace.stage(
        "gate.sx",
        "\
span := import span
pub eval := (start: i64, width: i64) -> i64 {
    window := span.Span { offset: start, length: width }
    span.measure(&window) + window.offset
}
",
    );
    workspace.stage(
        "modular.sx",
        "\
span := import span
gate := import gate
leaf := import nested.leaf
pub main := () -> i64 {
    base := span.Span { offset: 5, length: 12 }
    first := span.measure(&base) + base.length
    second := gate.eval(7, 9)
    (leaf.scale(first) + second) - (second / 9) - first
}
",
    )
}

fn arithmetic() -> &'static str {
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
"
}

fn control() -> &'static str {
    "\
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
"
}

fn metaprogramming() -> &'static str {
    "\
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
"
}
