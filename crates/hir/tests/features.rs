use simplex_hir::{Expression, Statement, Type, lower};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

static SERIAL: AtomicUsize = AtomicUsize::new(0);

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let id = SERIAL.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("simplex-feat-{label}-{}-{id}", process::id()));
        fs::create_dir_all(&dir).expect("mkdir");
        Self { dir }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn evaluation() {
    let constant = Path::new("../../cases/comptime/pass/constant.sx");
    let unit = lower("app", constant, false).expect("lower constant");
    assert!(
        unit.routines
            .iter()
            .any(|r| r.symbol == "sx$app$constant$align$4096" && r.inputs.len() == 1)
    );

    let derive = Path::new("../../cases/meta/pass/derive.sx");
    let derived = lower("app", derive, false).expect("lower derive");
    assert!(
        derived
            .tables
            .iter()
            .any(|t| t.symbol == "sx$vt$app$derive$Endpoint$Equal")
    );
    assert!(
        derived
            .tables
            .iter()
            .any(|t| t.symbol == "sx$vt$app$derive$Endpoint$Summary")
    );
}

#[test]
fn provenance() {
    let caller = Path::new("../../cases/meta/pass/caller.sx");
    let unit = lower("app", caller, false).expect("lower caller");
    assert!(unit.catalog.records.contains_key("Origin"));
    let rec = unit
        .routines
        .iter()
        .find(|r| r.symbol == "sx$app$caller$record")
        .expect("record routine");
    assert_eq!(rec.inputs.len(), 3);
    assert_eq!(rec.inputs[2].1, Type::Record(String::from("Origin")));

    let aspect = Path::new("../../cases/meta/pass/aspect.sx");
    let wrapped = lower("app", aspect, false).expect("lower aspect");
    let scale = wrapped
        .routines
        .iter()
        .find(|r| r.symbol == "sx$app$aspect$scale")
        .expect("scale");
    assert!(matches!(
        scale.body.last(),
        Some(Statement::Result(Expression::Call { symbol, .. })) if symbol == "sx$app$aspect$trace"
    ));
}

#[test]
fn assets() {
    let scratch = Scratch::new("assets");
    fs::create_dir_all(scratch.dir.join("rules")).expect("mkdir");
    fs::write(scratch.dir.join("rules/default.toml"), b"abc").expect("asset");
    let src = fs::read_to_string("../../cases/meta/pass/embed.sx").expect("read embed.sx");
    let entry = scratch.dir.join("embed.sx");
    fs::write(&entry, src).expect("write embed.sx");
    let unit = lower("app", &entry, false).expect("lower embed");
    assert!(unit.assets.iter().any(|a| a.bytes == b"abc"));
}

#[test]
fn destructors() {
    let scratch = Scratch::new("drop");
    let entry = scratch.dir.join("cleanup.sx");
    let raw = concat!(
        "Drop := interface {\n    drop: (self: &var Self)\n}\n\n",
        "Guard := struct {\n    tag: u8\n}\n\n",
        "Guard : Drop {\n    drop: (self: &var Guard) {\n        self.tag = 0\n    }\n}\n\n",
        "take := (item: Guard) -> u8 {\n    item.tag\n}\n\n",
        "pub main := () -> i64 {\n",
        "    var first := Guard { tag: 1 }\n",
        "    var second := Guard { tag: 2 }\n",
        "    var third := Guard { tag: 3 }\n",
        "    first.tag += 1\n",
        "    second.tag += 1\n",
        "    moved := take(third)\n",
        "    (first.tag + second.tag + moved + moved).into<i64>()\n",
        "}\n",
    );
    let formatted =
        simplex::render(&simplex::parser::parse(&simplex::tokenize(raw).unwrap()).unwrap());
    fs::write(&entry, formatted).expect("write cleanup");
    let unit = lower("app", &entry, false).expect("lower cleanup");
    let main = unit
        .routines
        .iter()
        .find(|r| r.symbol == "sx$app$cleanup$main")
        .expect("main");
    assert_eq!(unit.entry.as_deref(), Some("sx$app$cleanup$main"));
    let dropped: Vec<&str> = main
        .body
        .iter()
        .filter_map(|s| match s {
            Statement::Drop { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(dropped, vec!["second", "first"]);
}

#[test]
fn inferred() {
    let scratch = Scratch::new("inferred");
    let entry = scratch.dir.join("control.sx");
    let raw = concat!(
        "Step := enum {\n    more(i64)\n    done(i64)\n}\n\n",
        "Envelope := enum {\n    empty\n    boxed(i64, i64, i64, i64)\n}\n\n",
        "unpack := (item: Envelope) switch item {\n",
        "    Envelope.empty => 0\n",
        "    Envelope.boxed(a, b, c, d) => a + a + b + b + c + c + d + d\n",
        "}\n\n",
        "step := (cur: i64) bucket {\n",
        "    cur < 3 => Step.more(cur + 1)\n",
        "    _ => Step.done(cur * 10)\n",
        "}\n\n",
        "pub main := () -> i64 {\n",
        "    var cursor: i64 = 0\n",
        "    var box := Envelope.boxed(1, 2, 3, 4)\n",
        "    base := unpack(box) + measure(pace(2))\n",
        "    loop switch step(cursor + base - base) {\n",
        "        Step.done(v) => break v + v + base\n",
        "        Step.more(next) => {\n",
        "            cursor = next\n",
        "            continue\n",
        "        }\n",
        "    }\n",
        "}\n\n",
        "pace := (index: i64) bucket {\n",
        "    index < 1 => Step.done(0)\n",
        "    _ => Step.done(index * 2)\n",
        "}\n\n",
        "measure := (item: Step) switch item {\n",
        "    Step.more(n) => n + n\n",
        "    Step.done(n) => n + n\n",
        "}\n",
    );
    let formatted =
        simplex::render(&simplex::parser::parse(&simplex::tokenize(raw).unwrap()).unwrap());
    fs::write(&entry, formatted).expect("write control");
    let unit = lower("app", &entry, false).expect("lower control");
    let main = unit
        .routines
        .iter()
        .find(|r| r.symbol == "sx$app$control$main")
        .expect("main");
    assert_eq!(main.output, Type::Signed(8));
    assert!(matches!(main.body.last(), Some(Statement::Result(_))));
    let unpacked = main.body.iter().any(|s| {
        matches!(
            s,
            Statement::Bind {
                kind: Type::Signed(8),
                value: Expression::Binary {
                    left,
                    right,
                    ..
                },
                ..
            } if matches!(left.as_ref(), Expression::Call { kind: Type::Signed(8), .. })
                && matches!(right.as_ref(), Expression::Call { kind: Type::Signed(8), .. })
        )
    });
    assert!(unpacked);
}

#[test]
fn fibers() {
    let fiber = Path::new("../../cases/async/pass/fiber.sx");
    let unit = lower("app", fiber, false).expect("lower fiber");
    let pump = unit
        .routines
        .iter()
        .find(|r| r.symbol == "sx$app$fiber$pump")
        .expect("pump");
    assert!(pump.asynchronous);
    assert!(pump.body.iter().any(|s| matches!(
        s,
        Statement::Bind {
            value: Expression::All { .. },
            ..
        }
    )));

    let pipe = Path::new("../../cases/async/pass/pipeline.sx");
    let raced = lower("app", pipe, false).expect("lower pipeline");
    let fastest = raced
        .routines
        .iter()
        .find(|r| r.symbol == "sx$app$pipeline$fastest")
        .expect("fastest");
    assert!(fastest.body.iter().any(|s| matches!(
        s,
        Statement::Bind {
            value: Expression::Race { .. },
            ..
        }
    )));
}
