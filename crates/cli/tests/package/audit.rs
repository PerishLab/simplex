use crate::archive;
use crate::fixture;
use crate::harness::{Client, Sandbox};
use crate::server::Server;

fn manifest(version: &str) -> String {
    fixture::manifest("gate", version, "stable", "")
}

fn baseline(bias: u64) -> String {
    format!(
        "\
pub Frame := struct {{
    pub code: u64
    pub size: u64
}}
bias := () -> u64 {{
    {bias}
}}
pub compute := (frame: &Frame) -> u64 {{
    frame.code + frame.size + bias()
}}
"
    )
}

fn additive() -> &'static str {
    "\
pub Frame := struct {
    pub code: u64
    pub size: u64
}
bias := () -> u64 {
    9
}
pub compute := (frame: &Frame) -> u64 {
    frame.code + frame.size + bias()
}
pub expand := (value: u64) -> u64 {
    value * 2
}
"
}

fn broken() -> &'static str {
    "\
pub Frame := struct {
    pub code: u64
    pub size: u64
}
pub compute := (frame: &Frame, factor: u64) -> u64 {
    (frame.code + frame.size) * factor
}
pub expand := (value: u64) -> u64 {
    value * 2
}
"
}

fn reshaped() -> &'static str {
    "\
pub Frame := struct {
    pub code: u32
    pub size: u64
}
pub compute := (frame: &Frame) -> u64 {
    frame.code.into<u64>() + frame.size
}
pub expand := (value: u64) -> u64 {
    value * 2
}
"
}

fn stamp(server: &Server, version: &str) -> archive::Stamp {
    let key = format!("perish/gate/{version}/stable/seal.toml");
    let bytes = server.object(&key).expect("published seal");
    let text = String::from_utf8(bytes).expect("seal utf8");
    archive::parse(&text)
}

#[test]
fn evolution() {
    let server = Server::start("audit", "auto", false);
    let client = Client::signed(&server, "auto");
    let sandbox = Sandbox::open();

    sandbox.write("gate/simplex.toml", &manifest("1.0.0"));
    sandbox.write("gate/gate.sx", &baseline(3));
    let initial = sandbox.run(&["publish", "gate"], Some(&client));
    assert!(initial.passed, "1.0.0 failed: {}", initial.stderr);
    let first = stamp(&server, "1.0.0");

    sandbox.write("gate/simplex.toml", &manifest("1.0.1"));
    sandbox.write("gate/gate.sx", &baseline(9));
    let compatible = sandbox.run(&["publish", "gate"], Some(&client));
    assert!(
        compatible.passed,
        "1.0.1 internal edit failed: {}",
        compatible.stderr
    );
    let second = stamp(&server, "1.0.1");
    assert_ne!(first.source, second.source);
    assert_eq!(first.api, second.api);
    assert_eq!(first.abi, second.abi);

    sandbox.write("gate/simplex.toml", &manifest("1.0.2"));
    sandbox.write("gate/gate.sx", additive());
    let added = sandbox.run(&["publish", "gate"], Some(&client));
    assert!(
        added.passed,
        "1.0.2 additive export failed: {}",
        added.stderr
    );
    let third = stamp(&server, "1.0.2");
    assert_ne!(second.api, third.api);

    sandbox.write("gate/simplex.toml", &manifest("1.0.3"));
    sandbox.write("gate/gate.sx", broken());
    assert!(!sandbox.run(&["publish", "gate"], Some(&client)).passed);
    assert!(
        !sandbox
            .run(
                &["seal", "gate", "--baseline", "perish/gate/1.0.2/stable"],
                Some(&client)
            )
            .passed
    );
    assert!(
        server
            .object("perish/gate/1.0.3/stable/seal.toml")
            .is_none()
    );

    sandbox.write("gate/gate.sx", reshaped());
    assert!(!sandbox.run(&["publish", "gate"], Some(&client)).passed);
    assert!(
        server
            .object("perish/gate/1.0.3/stable/seal.toml")
            .is_none()
    );

    sandbox.write("gate/simplex.toml", &manifest("1.1.0"));
    let minor = sandbox.run(&["publish", "gate"], Some(&client));
    assert!(
        minor.passed,
        "1.1.0 breaking y bump failed: {}",
        minor.stderr
    );
    assert!(
        server
            .object("perish/gate/1.1.0/stable/seal.toml")
            .is_some()
    );

    sandbox.write("gate/simplex.toml", &manifest("2.0.0"));
    sandbox.write(
        "gate/gate.sx",
        "pub epochal := (seed: u64) -> u64 { seed + 100 }\n",
    );
    assert!(!sandbox.run(&["publish", "gate"], Some(&client)).passed);
    assert!(
        server
            .object("perish/gate/2.0.0/stable/seal.toml")
            .is_none()
    );

    let authorized = sandbox.run(&["publish", "gate", "--epoch"], Some(&client));
    assert!(
        authorized.passed,
        "2.0.0 --epoch failed: {}",
        authorized.stderr
    );
    assert!(
        server
            .object("perish/gate/2.0.0/stable/seal.toml")
            .is_some()
    );

    assert!(
        !sandbox
            .run(&["publish", "gate", "--epoch"], Some(&client))
            .passed
    );
    sandbox.write("gate/simplex.toml", &manifest("1.9.0"));
    assert!(!sandbox.run(&["publish", "gate"], Some(&client)).passed);
    assert!(
        !sandbox
            .run(&["publish", "gate", "--epoch"], Some(&client))
            .passed
    );
    assert!(
        server
            .object("perish/gate/1.9.0/stable/seal.toml")
            .is_none()
    );
}
