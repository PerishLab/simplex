use simplex_hir::translate;
use simplex_ir::{Module, Operation, Type, prepare, verify};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

static SERIAL: AtomicUsize = AtomicUsize::new(0);

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let id = SERIAL.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("simplex-coro-{label}-{}-{id}", process::id()));
        fs::create_dir_all(&dir).expect("mkdir");
        Self { dir }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn validate(mut module: Module) -> Module {
    verify(&module).expect("verify ir");
    let snapshot = module.clone();
    prepare(&mut module).expect("prepare ir");
    verify(&module).expect("verify prepared");
    snapshot
}

#[test]
fn cancellation() {
    let scratch = Scratch::new("cancel");
    let entry = scratch.dir.join("race.sx");
    let raw = concat!(
        "emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize\n\n",
        "Drop := interface {\n    drop: (self: &var Self)\n}\n\n",
        "Guard := struct {\n    tag: u8\n}\n\n",
        "Guard : Drop {\n    drop: (self: &var Guard) {\n        emit(1, &self.tag, 1)\n    }\n}\n\n",
        "slow := async (first: u8, second: u8) -> u64 {\n",
        "    var outer := Guard { tag: first + first - first }\n",
        "    var inner := Guard { tag: second + second - second }\n",
        "    yield()\n",
        "    (outer.tag + outer.tag + inner.tag + inner.tag).into<u64>()\n",
        "}\n\n",
        "fast := async (winner: u64) -> u64 {\n    winner + winner - winner\n}\n\n",
        "pub main := async () -> i64 {\n",
        "    var scoped := Guard { tag: 9 }\n",
        "    won := [slow(1, 2), fast(77)].race().await + scoped.tag.into<u64>() - scoped.tag.into<u64>()\n",
        "    (won + won - won).as<i64>()\n",
        "}\n",
    );
    let formatted =
        simplex::render(&simplex::parser::parse(&simplex::tokenize(raw).unwrap()).unwrap());
    fs::write(&entry, formatted).expect("write");
    let module = validate(translate("app", &entry, false).expect("translate"));
    let slow = module
        .functions
        .iter()
        .find(|func| func.symbol == "sx$app$race$slow")
        .expect("slow");
    assert!(slow.asynchronous);
    assert_eq!(slow.cleanup.as_deref(), Some("sx$clean$sx$app$race$slow"));
    assert!(matches!(
        slow.blocks[0].body.first().map(|inst| &inst.op),
        Some(Operation::Slot { align: 8, .. })
    ));
    let cleaner = module
        .functions
        .iter()
        .find(|func| func.symbol == "sx$clean$sx$app$race$slow")
        .expect("cleaner");
    let offsets: Vec<i64> = cleaner
        .blocks
        .iter()
        .flat_map(|block| &block.body)
        .filter_map(|inst| match inst.op {
            Operation::Offset { bytes, .. } => Some(bytes),
            _ => None,
        })
        .collect();
    assert_eq!(offsets, vec![16, 8]);
}

#[test]
fn gather() {
    let scratch = Scratch::new("gather");
    let entry = scratch.dir.join("gather.sx");
    let raw = concat!(
        "scale := async (seed: u64) -> u64 {\n",
        "    yield()\n",
        "    seed * 5 + (seed - seed)\n",
        "}\n\n",
        "pub main := async () -> i64 {\n",
        "    seeds: List<u64> = [2, 4, 6]\n",
        "    mapped := seeds.map((item) { scale(item + item - item) }).all().await\n",
        "    (mapped[0] + mapped[1] + mapped[2] + (seeds.len() - 3).into<u64>()).as<i64>()\n",
        "}\n",
    );
    let formatted =
        simplex::render(&simplex::parser::parse(&simplex::tokenize(raw).unwrap()).unwrap());
    fs::write(&entry, formatted).expect("write");
    let module = validate(translate("app", &entry, false).expect("translate"));
    let main = module
        .functions
        .iter()
        .find(|func| func.symbol == "sx$app$gather$main")
        .expect("main");
    assert!(main.asynchronous);
    assert!(
        main.blocks
            .iter()
            .flat_map(|b| &b.body)
            .any(|inst| { matches!((&inst.kind, &inst.op), (Type::Pair, Operation::All { .. })) })
    );
}

#[test]
fn dispatch() {
    let scratch = Scratch::new("dispatch");
    let codec = concat!(
        "pub dylib Codec := interface {\n",
        "    encode: (self: &Self, input: &[u8]) -> usize\n",
        "}\n\n",
        "pub dylib run := (engine: &Codec, input: &[u8]) -> usize {\n",
        "    got := engine.encode(input)\n",
        "    got + (got - got) + input.len() - input.len()\n",
        "}\n",
    );
    let main = concat!(
        "codec := dylib codec\n\n",
        "Cipher := struct {\n    key: u8\n}\n\n",
        "Cipher : codec.Codec {\n",
        "    encode: (self: &Cipher, input: &[u8]) -> usize {\n",
        "        (input[0] ^ self.key ^ self.key).into<usize>() + input.len()\n",
        "    }\n",
        "}\n\n",
        "pub main := () -> i64 {\n",
        "    cipher := Cipher { key: 5 }\n",
        "    var bytes: List<u8> = [12, 9]\n",
        "    first := codec.run(&cipher, &bytes[0..2]) + codec.run(&cipher, &bytes[0..2])\n",
        "    (first + first - first + bytes[1].into<usize>()).as<i64>()\n",
        "}\n",
    );
    fs::write(
        scratch.dir.join("codec.sx"),
        simplex::render(&simplex::parser::parse(&simplex::tokenize(codec).unwrap()).unwrap()),
    )
    .expect("write codec");
    let entry = scratch.dir.join("main.sx");
    fs::write(
        &entry,
        simplex::render(&simplex::parser::parse(&simplex::tokenize(main).unwrap()).unwrap()),
    )
    .expect("write main");
    let shared = validate(translate("app", &scratch.dir.join("codec.sx"), true).expect("dylib"));
    assert!(shared.shared);
    assert_eq!(shared.entry, None);
    let host = validate(translate("app", &entry, false).expect("host"));
    assert!(!host.shared);
    assert_eq!(host.entry.as_deref(), Some("sx$app$main$main"));
}

#[test]
fn recycling() {
    let scratch = Scratch::new("recycle");
    let entry = scratch.dir.join("recycle.sx");
    let raw = concat!(
        "Step := enum {\n    done\n    more\n}\n\n",
        "Packet := struct {\n    code: usize\n    size: usize\n}\n\n",
        "pace := (index: u64) bucket {\n",
        "    index >= 8 => Step.done\n",
        "    _ => Step.more\n",
        "}\n\n",
        "pack := async (seed: usize) -> Packet {\n",
        "    yield()\n",
        "    Packet { code: seed * 2, size: seed + 3 }\n",
        "}\n\n",
        "round := async (index: u64) -> u64 {\n",
        "    item := pack(index.as<usize>()).await\n",
        "    (item.code + item.size + item.size - item.size).as<u64>() + index - index\n",
        "}\n\n",
        "pub main := async () -> i64 {\n",
        "    var index: u64 = 0\n",
        "    var total: u64 = 0\n",
        "    loop switch pace(index) {\n",
        "        Step.done => break (total + total - total).as<i64>()\n",
        "        Step.more => {\n",
        "            total += round(index).await\n",
        "            index += 1\n",
        "        }\n",
        "    }\n",
        "}\n",
    );
    let formatted =
        simplex::render(&simplex::parser::parse(&simplex::tokenize(raw).unwrap()).unwrap());
    fs::write(&entry, formatted).expect("write");
    let module = validate(translate("app", &entry, false).expect("translate"));
    assert!(
        module
            .functions
            .iter()
            .any(|func| func.symbol == "sx$app$recycle$pack" && func.asynchronous)
    );
}
