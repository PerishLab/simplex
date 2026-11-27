use crate::fixture;
use crate::harness::{Client, Sandbox};
use crate::server::Server;
use std::fs;

pub fn consumer(sandbox: &Sandbox, channel: &str) {
    let mut dependencies = fixture::exact("json", "codec.json", "stable");
    dependencies.push_str(&fixture::exact("math", "math", channel));
    sandbox.write(
        "app/simplex.toml",
        &fixture::manifest("app", "0.1.0", "stable", &dependencies),
    );
    sandbox.write(
        "app/main.sx",
        "\
parse := import json.parse
metric := dylib math.metric
emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize
Scale := struct {
    factor: u64
}
Scale : metric.Metric {
    weigh: (self: &Scale, pair: &metric.Pair) -> u64 {
        (pair.left + pair.right) * self.factor
    }
}
pub main := () -> i64 {
    var banner: List<u8> = [79, 75]
    scale := Scale { factor: 2 }
    pair := metric.Pair { left: 6, right: 9 }
    emit(1, &banner[0], 2).as<i64>() + (parse.scan(10) + metric.combine(&scale, &pair) + scale.factor + pair.left).as<i64>()
}
",
    );
}

pub fn upstream(sandbox: &Sandbox, client: &Client) {
    fixture::stage(sandbox);
    fixture::math(sandbox, "math", "canary", 5);
    fixture::publish(
        sandbox,
        client,
        &[
            "workspace/lemmas/wire",
            "workspace/lemmas/codec/json",
            "math",
        ],
    );
}

#[test]
fn fallback() {
    let server = Server::start("channels", "auto", false);
    let client = Client::signed(&server, "auto");
    let mut sandbox = Sandbox::open();
    upstream(&sandbox, &client);
    sandbox.fresh();
    consumer(&sandbox, "stable");
    let binary = sandbox.join("out/app");
    let missing = sandbox.run(
        &["build", "app", "-o", binary.to_str().expect("out path")],
        Some(&client),
    );
    assert!(
        !missing.passed,
        "channel mismatch must never fall back to canary"
    );
    assert!(!binary.exists());
}

#[test]
fn pipeline() {
    let mut server = Server::start("public", "auto", true);
    let writer = Client::signed(&server, "auto");
    let reader = Client::unsigned(&server, "auto");
    let mut sandbox = Sandbox::open();
    fixture::math(&sandbox, "math", "canary", 5);
    assert!(!sandbox.run(&["publish", "math"], Some(&reader)).passed);
    upstream(&sandbox, &writer);

    sandbox.fresh();
    consumer(&sandbox, "canary");
    let folder = sandbox.join("online");
    let binary = folder.join("app");
    let built = sandbox.run(
        &["build", "app", "-o", binary.to_str().expect("binary")],
        Some(&reader),
    );
    assert!(
        built.passed,
        "public unsigned build failed: {}",
        built.stderr
    );
    assert!(
        server
            .events()
            .iter()
            .any(|event| event.method == "GET" && !event.signed && event.status == 200)
    );
    let library = format!("{}.so", fixture::stem("math", "1.0.0", "canary"));
    assert_eq!(
        fixture::listing(&folder),
        fixture::sorted(vec![String::from("app"), library.clone()])
    );
    let run = sandbox.execute(&binary, &folder);
    assert_eq!(run.code, 105);
    assert_eq!(run.stdout, b"OK");

    server.stop();
    let second = sandbox.join("offline");
    let offline = second.join("app");
    let cached = sandbox.run(
        &[
            "build",
            "app/simplex.toml",
            "-o",
            offline.to_str().expect("offline"),
        ],
        None,
    );
    assert!(
        cached.passed,
        "offline cache build failed: {}",
        cached.stderr
    );
    assert_eq!(
        fixture::listing(&second),
        fixture::sorted(vec![String::from("app"), library])
    );
    let rerun = sandbox.execute(&offline, &second);
    assert_eq!(rerun.code, 105);
    assert_eq!(rerun.stdout, b"OK");

    let entry = sandbox.join("entry/app");
    let third = sandbox.run(
        &["build", "app/main.sx", "-o", entry.to_str().expect("entry")],
        None,
    );
    assert!(third.passed, "main.sx build failed: {}", third.stderr);

    let archive = sandbox
        .home()
        .join("cache/perish/math/1.0.0/canary/source.tar");
    let mut bytes = fs::read(&archive).expect("cached source.tar");
    bytes[0] ^= 0xff;
    fs::write(&archive, &bytes).expect("corrupt cached source.tar");
    let tampered = sandbox.join("tampered/app");
    let refused = sandbox.run(
        &["build", "app", "-o", tampered.to_str().expect("tampered")],
        None,
    );
    assert!(!refused.passed);
}

fn side(alias: &str, field: &str) -> String {
    format!(
        "\
metric := dylib {alias}.metric
Unit := struct {{
    weight: u64
}}
Unit : metric.Metric {{
    weigh: (self: &Unit, pair: &metric.Pair) -> u64 {{
        pair.left + pair.right + self.weight
    }}
}}
pub score := () -> u64 {{
    unit := Unit {{ weight: 3 }}
    pair := metric.Pair {{ left: 4, right: 6 }}
    metric.combine(&unit, &pair) + pair.{field} + unit.weight
}}
"
    )
}

#[test]
fn coexistence() {
    let server = Server::start("multi", "auto", false);
    let client = Client::signed(&server, "auto");
    let mut sandbox = Sandbox::open();
    fixture::math(&sandbox, "canary", "canary", 5);
    fixture::math(&sandbox, "stable", "stable", 20);
    fixture::publish(&sandbox, &client, &["canary", "stable"]);

    sandbox.fresh();
    let mut dependencies = fixture::exact("alpha", "math", "canary");
    dependencies.push_str(&fixture::exact("beta", "math", "stable"));
    sandbox.write(
        "dual/simplex.toml",
        &fixture::manifest("dual", "0.1.0", "stable", &dependencies),
    );
    sandbox.write("dual/left.sx", &side("alpha", "left"));
    sandbox.write("dual/right.sx", &side("beta", "right"));
    sandbox.write(
        "dual/main.sx",
        "\
left := import left
right := import right
pub main := () -> i64 {
    (left.score() + right.score()).as<i64>()
}
",
    );
    let folder = sandbox.join("output");
    let binary = folder.join("dual");
    let built = sandbox.run(
        &["build", "dual", "-o", binary.to_str().expect("dual binary")],
        Some(&client),
    );
    assert!(built.passed, "dual channel build failed: {}", built.stderr);
    let canary = fixture::stem("math", "1.0.0", "canary");
    let stable = fixture::stem("math", "1.0.0", "stable");
    assert_ne!(canary, stable);
    for stem in [&canary, &stable] {
        assert_eq!(stem.len(), 16);
        assert!(
            stem.bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        );
    }
    let expected = fixture::sorted(vec![
        format!("{canary}.so"),
        format!("{stable}.so"),
        String::from("dual"),
    ]);
    assert_eq!(fixture::listing(&folder), expected);
    let executed = sandbox.execute(&binary, &folder);
    assert_eq!(executed.code, 87);
}
