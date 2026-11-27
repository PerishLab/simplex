use crate::fixture;
use crate::harness::{Client, Sandbox};
use crate::server::Server;
use std::fs;

const MAIN: &str = "\
metric := dylib math.metric
Flat := struct {
    weight: u64
}
Flat : metric.Metric {
    weigh: (self: &Flat, pair: &metric.Pair) -> u64 {
        self.weight + pair.left
    }
}
pub main := () -> i64 {
    flat := Flat { weight: 4 }
    pair := metric.Pair { left: 1, right: 2 }
    (metric.combine(&flat, &pair) + flat.weight + pair.right).as<i64>()
}
";

const BRIDGE: &str = "\
metric := dylib math.metric
Flat := struct {
    weight: u64
}
Flat : metric.Metric {
    weigh: (self: &Flat, pair: &metric.Pair) -> u64 {
        self.weight + pair.left
    }
}
pub dylib total := (value: u64) -> u64 {
    flat := Flat { weight: value }
    pair := metric.Pair { left: 1, right: 2 }
    metric.combine(&flat, &pair) + flat.weight + pair.right
}
";

fn prepare() -> (Server, Client, Sandbox) {
    let server = Server::start("collide", "auto", false);
    let client = Client::signed(&server, "auto");
    let mut sandbox = Sandbox::open();
    fixture::math(&sandbox, "math", "canary", 5);
    fixture::publish(&sandbox, &client, &["math"]);
    sandbox.fresh();
    let dependencies = fixture::exact("math", "math", "canary");
    for (name, file, source) in [("app", "main.sx", MAIN), ("relay", "bridge.sx", BRIDGE)] {
        sandbox.write(
            &format!("{name}/simplex.toml"),
            &fixture::manifest(name, "0.1.0", "stable", &dependencies),
        );
        sandbox.write(&format!("{name}/{file}"), source);
    }
    (server, client, sandbox)
}

fn clash(sandbox: &Sandbox, client: &Client, arguments: &[&str], folder: &str) {
    let library = format!("{}.so", fixture::stem("math", "1.0.0", "canary"));
    let target = sandbox.join(&format!("{folder}/{library}"));
    let output = target.to_str().expect("collision path");
    let mut command = arguments.to_vec();
    command.extend(["-o", output]);
    let fresh = sandbox.run(&command, Some(client));
    assert!(!fresh.passed, "{command:?} overwrote {library}");
    assert_ne!(fresh.code, 0);
    assert!(fresh.stderr.contains("collision"), "{}", fresh.stderr);
    assert!(!target.exists());

    sandbox.asset(&format!("{folder}/{library}"), b"sentinel");
    let occupied = sandbox.run(&command, Some(client));
    assert!(!occupied.passed, "{command:?} overwrote existing {library}");
    assert!(occupied.stderr.contains("collision"), "{}", occupied.stderr);
    assert_eq!(fs::read(&target).expect("sentinel"), b"sentinel");
}

#[test]
fn executable() {
    let (server, client, sandbox) = prepare();
    let binary = sandbox.join("control/app");
    let built = sandbox.run(
        &["build", "app", "-o", binary.to_str().expect("binary")],
        Some(&client),
    );
    assert!(built.passed, "control build failed: {}", built.stderr);
    let library = format!("{}.so", fixture::stem("math", "1.0.0", "canary"));
    let folder = sandbox.join("control");
    assert_eq!(
        fixture::listing(&folder),
        fixture::sorted(vec![String::from("app"), library])
    );
    assert_eq!(sandbox.execute(&binary, &folder).code, 19);
    clash(&sandbox, &client, &["build", "app"], "first");
    assert_eq!(server.keys().len(), 2);
}

#[test]
fn shared() {
    let (server, client, sandbox) = prepare();
    let control = sandbox.join("control/relay.so");
    let built = sandbox.run(
        &[
            "build",
            "relay",
            "--dylib",
            "-o",
            control.to_str().expect("relay"),
        ],
        Some(&client),
    );
    assert!(built.passed, "control dylib build failed: {}", built.stderr);
    assert!(control.exists());
    clash(&sandbox, &client, &["build", "relay", "--dylib"], "second");
    assert_eq!(server.keys().len(), 2);
}
