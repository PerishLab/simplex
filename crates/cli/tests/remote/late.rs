use crate::armor::Record;
use crate::runner::Workspace;
use crate::standalone::{Case, record};

pub fn expected() -> Vec<Record> {
    vec![
        record("sequential", 80, b""),
        record("gather", 93, b"ABCD"),
        record("cancellation", 77, b"abW4321"),
        record("recycling", 128, b""),
        record("initial", 94, b""),
        record("swapped", 130, b""),
    ]
}

pub fn stage(workspace: &Workspace) -> Vec<Case> {
    let first = workspace.stage("sequential.sx", sequential());
    let second = workspace.stage("gather.sx", gather());
    let third = workspace.stage("cancellation.sx", cancellation());
    let fourth = workspace.stage("recycling.sx", recycling());
    vec![
        Case {
            label: "sequential",
            entry: first,
            expected: record("sequential", 80, b""),
        },
        Case {
            label: "gather",
            entry: second,
            expected: record("gather", 93, b"ABCD"),
        },
        Case {
            label: "cancellation",
            entry: third,
            expected: record("cancellation", 77, b"abW4321"),
        },
        Case {
            label: "recycling",
            entry: fourth,
            expected: record("recycling", 128, b""),
        },
    ]
}

pub fn library(bias: u64, salt: u8) -> String {
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

pub fn caller() -> &'static str {
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
"
}

fn sequential() -> &'static str {
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
"
}

fn gather() -> &'static str {
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
"
}

fn cancellation() -> &'static str {
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
"
}

fn recycling() -> &'static str {
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
"
}
