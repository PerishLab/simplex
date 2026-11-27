use crate::harness::{self, Client, Sandbox};
use crate::server::Server;
use std::fs;

pub fn consumer(sandbox: &Sandbox, channel: &str) {
    sandbox.write(
        "app/simplex.toml",
        &format!(
            "\
schema = \"simplex.package/v1\"

[library]
scope = \"perish\"
name = \"app\"
version = \"0.1.0\"
channel = \"stable\"
entry = \"app.sx\"

[dependencies]
codec = {{ scope = \"perish\", name = \"codec\", version = \"1.0.0\", channel = \"stable\" }}
math = {{ scope = \"perish\", name = \"math\", version = \"1.0.0\", channel = \"{channel}\" }}
"
        ),
    );
    sandbox.write(
        "app/app.sx",
        "\
codec := import codec
math := dylib math
emit := extern \"c\" \"write\" fn(handle: i32, buffer: &u8, count: usize) -> isize
Scale := struct {
    factor: u64
}
Scale : math.Metric {
    weigh: (self: &Scale, pair: &math.Pair) -> u64 {
        (pair.left + pair.right) * self.factor
    }
}
pub main := () -> i64 {
    var banner: List<u8> = [79, 75]
    scale := Scale { factor: 2 }
    pair := math.Pair { left: 6, right: 9 }
    emit(1, &banner[0], 2).as<i64>() + (codec.stamp(10) + math.combine(&scale, &pair) + scale.factor + pair.left).as<i64>()
}
",
    );
}

#[test]
fn fallback() {
    let server = Server::start("channels", "auto", false);
    let client = Client::signed(&server, "auto");
    let mut sandbox = Sandbox::open();
    harness::stage(&sandbox);
    harness::math(&sandbox, "math", "canary", 5);

    assert!(
        sandbox
            .run(&["publish", "workspace/wire"], Some(&client))
            .passed
    );
    assert!(
        sandbox
            .run(&["publish", "workspace/codec"], Some(&client))
            .passed
    );
    assert!(sandbox.run(&["publish", "math"], Some(&client)).passed);

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
}

#[test]
fn pipeline() {
    let mut server = Server::start("public", "auto", true);
    let writer = Client::signed(&server, "auto");
    let reader = Client::unsigned(&server, "auto");
    let mut sandbox = Sandbox::open();
    harness::stage(&sandbox);
    harness::math(&sandbox, "math", "canary", 5);

    assert!(!sandbox.run(&["publish", "math"], Some(&reader)).passed);
    assert!(
        sandbox
            .run(&["publish", "workspace/wire"], Some(&writer))
            .passed
    );
    assert!(
        sandbox
            .run(&["publish", "workspace/codec"], Some(&writer))
            .passed
    );
    assert!(sandbox.run(&["publish", "math"], Some(&writer)).passed);

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
    assert!(binary.exists());
    assert!(folder.join("libsx_perish_math_1_0_0_canary.so").exists());
    assert!(!folder.join("libsx_perish_codec_1_0_0_stable.so").exists());
    let run = sandbox.execute(&binary, &folder);
    assert_eq!(run.code, 105);
    assert_eq!(run.stdout, b"OK");

    server.stop();
    let offline = sandbox.join("offline/app");
    let cached = sandbox.run(
        &[
            "build",
            "app/simplex.toml",
            "-o",
            offline.to_str().expect("offline binary"),
        ],
        None,
    );
    assert!(
        cached.passed,
        "offline cache build failed: {}",
        cached.stderr
    );
    let second = sandbox.join("offline");
    assert!(second.join("libsx_perish_math_1_0_0_canary.so").exists());
    let rerun = sandbox.execute(&offline, &second);
    assert_eq!(rerun.code, 105);
    assert_eq!(rerun.stdout, b"OK");

    let entry = sandbox.join("entry/app");
    let third = sandbox.run(
        &[
            "build",
            "app/app.sx",
            "-o",
            entry.to_str().expect("entry binary"),
        ],
        Some(&reader),
    );
    assert!(third.passed, "entry .sx build failed: {}", third.stderr);

    let archive = sandbox
        .home()
        .join("cache/perish/math/1.0.0/canary/source.tar");
    let mut bytes = fs::read(&archive).expect("cached source.tar");
    bytes[0] ^= 0xff;
    fs::write(&archive, &bytes).expect("corrupt cached source.tar");
    let tampered = sandbox.join("tampered/app");
    assert!(
        !sandbox
            .run(
                &["build", "app", "-o", tampered.to_str().expect("tampered")],
                None
            )
            .passed
    );
}

#[test]
fn coexistence() {
    let server = Server::start("multi", "auto", false);
    let client = Client::signed(&server, "auto");
    let mut sandbox = Sandbox::open();
    harness::math(&sandbox, "canary", "canary", 5);
    harness::math(&sandbox, "stable", "stable", 20);
    assert!(sandbox.run(&["publish", "canary"], Some(&client)).passed);
    assert!(sandbox.run(&["publish", "stable"], Some(&client)).passed);

    sandbox.fresh();
    sandbox.write(
        "dual/simplex.toml",
        "\
schema = \"simplex.package/v1\"

[library]
scope = \"perish\"
name = \"dual\"
version = \"0.1.0\"
channel = \"stable\"
entry = \"dual.sx\"

[dependencies]
alpha = { scope = \"perish\", name = \"math\", version = \"1.0.0\", channel = \"canary\" }
beta = { scope = \"perish\", name = \"math\", version = \"1.0.0\", channel = \"stable\" }
",
    );
    sandbox.write(
        "dual/dual.sx",
        "\
alpha := dylib alpha
beta := dylib beta
Unit := struct {
    weight: u64
}
Unit : alpha.Metric {
    weigh: (self: &Unit, pair: &alpha.Pair) -> u64 {
        pair.left + pair.right + self.weight
    }
}
Unit : beta.Metric {
    weigh: (self: &Unit, pair: &beta.Pair) -> u64 {
        pair.left + pair.right + self.weight
    }
}
pub main := () -> i64 {
    unit := Unit { weight: 3 }
    left := alpha.Pair { left: 4, right: 6 }
    right := beta.Pair { left: 4, right: 6 }
    (alpha.combine(&unit, &left) + left.left + beta.combine(&unit, &right) + right.right).as<i64>()
}
",
    );
    let folder = sandbox.join("dual-out");
    let binary = folder.join("dual");
    let built = sandbox.run(
        &["build", "dual", "-o", binary.to_str().expect("dual binary")],
        Some(&client),
    );
    assert!(built.passed, "dual channel build failed: {}", built.stderr);
    assert!(folder.join("libsx_perish_math_1_0_0_canary.so").exists());
    assert!(folder.join("libsx_perish_math_1_0_0_stable.so").exists());
    let executed = sandbox.execute(&binary, &folder);
    assert_eq!(executed.code, 81);
}
