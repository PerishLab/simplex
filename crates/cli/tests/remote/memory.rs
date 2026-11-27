use crate::armor::Record;
use crate::runner::Workspace;
use crate::standalone::{Case, record};

pub fn expected() -> Vec<Record> {
    vec![
        record("aggregates", 78, b""),
        record("slices", 95, b""),
        record("assets", 107, b""),
        record("cleanup", 16, b"43!521"),
        record("external", 15, b"simplex-abi\nok!"),
    ]
}

pub fn stage(workspace: &Workspace) -> Vec<Case> {
    workspace.asset("assets/weights.bin", &[11, 22, 33, 44, 55]);
    workspace.asset("data/banner.txt", b"simplex-abi\n");
    let first = workspace.stage("aggregates.sx", aggregates());
    let second = workspace.stage("slices.sx", slices());
    let third = workspace.stage("assets.sx", assets());
    let fourth = workspace.stage("cleanup.sx", cleanup());
    let fifth = workspace.stage("external.sx", external());
    vec![
        Case {
            label: "aggregates",
            entry: first,
            expected: record("aggregates", 78, b""),
        },
        Case {
            label: "slices",
            entry: second,
            expected: record("slices", 95, b""),
        },
        Case {
            label: "assets",
            entry: third,
            expected: record("assets", 107, b""),
        },
        Case {
            label: "cleanup",
            entry: fourth,
            expected: record("cleanup", 16, b"43!521"),
        },
        Case {
            label: "external",
            entry: fifth,
            expected: record("external", 15, b"simplex-abi\nok!"),
        },
    ]
}

fn aggregates() -> &'static str {
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
"
}

fn slices() -> &'static str {
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
"
}

fn assets() -> &'static str {
    "\
weights := embed(\"assets/weights.bin\")
pub main := () -> i64 {
    window := &weights[1..4]
    (weights.len() + weights[0].into<usize>() + window[1].into<usize>() + window.len() + weights[4].into<usize>()).as<i64>()
}
"
}

fn cleanup() -> &'static str {
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
"
}

fn external() -> &'static str {
    "\
banner := embed(\"data/banner.txt\")
emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize
pub main := () -> i64 {
    var tail: List<u8> = [111, 107, 33]
    first := emit(1, &banner[0], banner.len())
    second := emit(1, &tail[0], 3)
    (first + second + (first - 12) + (second - 3)).as<i64>()
}
"
}
