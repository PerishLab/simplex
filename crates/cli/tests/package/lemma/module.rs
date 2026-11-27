use super::{UNIT, attempt, refuse};
use crate::fixture;
use crate::harness::{Client, Sandbox};
use crate::server::Server;
use std::fs;

const CONTROL: &str = "\
parse := import json.parse
pub main := () -> i64 {
    (parse.scan(1) + 2).as<i64>()
}
";

const MAIN: &str = "\
parse := import json.parse
format := dylib json.format
emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize
Scale := struct {
    factor: u64
}
Scale : format.Metric {
    weigh: (self: &Scale, pair: &format.Pair) -> u64 {
        (pair.left + pair.right) * self.factor
    }
}
pub main := () -> i64 {
    var banner: List<u8> = [79, 75]
    scale := Scale { factor: 2 }
    pair := format.Pair { left: 6, right: 9 }
    emit(1, &banner[0], 2).as<i64>() + (parse.scan(10) + format.combine(&scale, &pair) + scale.factor + pair.left).as<i64>()
}
";

fn prepare() -> (Server, Client, Sandbox) {
    let server = Server::start("modules", "auto", false);
    let client = Client::signed(&server, "auto");
    let mut sandbox = Sandbox::open();
    fixture::stage(&sandbox);
    fixture::publish(
        &sandbox,
        &client,
        &["workspace/lemmas/wire", "workspace/lemmas/codec/json"],
    );
    sandbox.fresh();
    let dependencies = fixture::exact("json", "codec.json", "stable");
    sandbox.write(
        "app/simplex.toml",
        &fixture::manifest("app", "0.1.0", "stable", &dependencies),
    );
    (server, client, sandbox)
}

fn build(sandbox: &Sandbox, client: &Client, folder: &str) -> bool {
    let binary = sandbox.join(&format!("{folder}/app"));
    let outcome = sandbox.run(
        &["build", "app", "-o", binary.to_str().expect("binary")],
        Some(client),
    );
    assert_eq!(outcome.passed, binary.exists(), "{}", outcome.stderr);
    outcome.passed
}

#[test]
fn directory() {
    let (server, client, sandbox) = prepare();
    assert_eq!(server.keys().len(), 4);
    sandbox.write("app/main.sx", MAIN);
    assert!(build(&sandbox, &client, "out"));
    let folder = sandbox.join("out");
    let stem = fixture::stem("codec.json", "1.0.0", "stable");
    assert_eq!(stem.len(), 16);
    let library = format!("{stem}.so");
    assert_eq!(
        fixture::listing(&folder),
        fixture::sorted(vec![String::from("app"), library.clone()])
    );
    let bytes = fs::read(folder.join(&library)).expect("lemma dylib");
    let symbol = format!("sx${stem}$format$combine");
    assert!(
        bytes
            .windows(symbol.len())
            .any(|part| part == symbol.as_bytes())
    );
    let host = fs::read(folder.join("app")).expect("host binary");
    let needed = format!("{library}\0");
    assert!(
        host.windows(needed.len())
            .any(|part| part == needed.as_bytes())
    );
    let run = sandbox.execute(&folder.join("app"), &folder);
    assert_eq!(run.code, 105);
    assert_eq!(run.stdout, b"OK");
}

#[test]
fn boundary() {
    let (server, client, sandbox) = prepare();
    sandbox.write("app/main.sx", CONTROL);
    assert!(build(&sandbox, &client, "control"));
    let replacements = ["json := import json\n", "json := dylib json\n"];
    for (index, line) in replacements.into_iter().enumerate() {
        let text = CONTROL
            .replace("parse := import json.parse\n", line)
            .replace("parse.scan", "json.scan");
        sandbox.write("app/main.sx", &text);
        assert!(
            !build(&sandbox, &client, &format!("single{index}")),
            "{line}"
        );
    }
    sandbox.write("app/main.sx", CONTROL);
    for shadow in ["app/json.sx", "app/json/foo.sx"] {
        sandbox.write(shadow, UNIT);
        assert!(
            !build(&sandbox, &client, "shadow"),
            "{shadow} shadowed json"
        );
        for verb in ["seal", "publish"] {
            assert!(
                !sandbox.run(&[verb, "app"], Some(&client)).passed,
                "{verb} {shadow}"
            );
        }
        fs::remove_file(sandbox.join(shadow)).expect("remove shadow");
    }
    fs::remove_dir(sandbox.join("app/json")).expect("remove shadow folder");
    assert!(build(&sandbox, &client, "restored"));
    assert!(server.object("perish/app/0.1.0/stable/seal.toml").is_none());
}

#[test]
fn barrel() {
    let server = Server::start("barrels", "auto", false);
    let client = Client::signed(&server, "auto");
    let sandbox = Sandbox::open();
    sandbox.write(
        "barrel/simplex.toml",
        &fixture::manifest("barrel", "1.0.0", "stable", ""),
    );
    sandbox.write("barrel/unit.sx", UNIT);
    let control = "pub other := () -> u64 { 2 }\n";
    for barrel in [
        "index.sx",
        "mod.sx",
        "lib.sx",
        "nested/mod.sx",
        "nested/index.sx",
    ] {
        let path = format!("barrel/{barrel}");
        sandbox.write(&path, control);
        refuse(&sandbox, &client, "barrel");
        fs::remove_file(sandbox.join(&path)).expect("remove barrel");
    }
    assert!(server.keys().is_empty());
    sandbox.write("barrel/nested/other.sx", control);
    for outcome in attempt(&sandbox, &client, "barrel") {
        assert!(
            outcome.passed,
            "barrel-free lemma refused: {}",
            outcome.stderr
        );
    }
}
