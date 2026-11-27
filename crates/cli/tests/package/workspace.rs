use crate::archive;
use crate::crypto;
use crate::harness::{self, Client, Sandbox};
use crate::server::{Event, Server};
use std::fs;

#[test]
fn rewrite() {
    let server = Server::start("simplex", "auto", false);
    let client = Client::signed(&server, "auto");
    let sandbox = Sandbox::open();
    harness::stage(&sandbox);

    let premature = sandbox.run(&["publish", "workspace/codec"], Some(&client));
    assert!(!premature.passed, "codec published before wire");
    assert!(server.keys().is_empty());

    let first = sandbox.run(&["publish", "workspace/wire"], Some(&client));
    assert!(first.passed, "wire publish failed: {}", first.stderr);
    assert_eq!(
        server.keys(),
        vec![
            String::from("perish/wire/1.0.0/stable/seal.toml"),
            String::from("perish/wire/1.0.0/stable/source.tar"),
        ]
    );
    verify(&server, "perish/wire/1.0.0/stable");

    let second = sandbox.run(&["publish", "workspace/codec"], Some(&client));
    assert!(second.passed, "codec publish failed: {}", second.stderr);
    assert_eq!(
        server.keys(),
        vec![
            String::from("perish/codec/1.0.0/stable/seal.toml"),
            String::from("perish/codec/1.0.0/stable/source.tar"),
            String::from("perish/wire/1.0.0/stable/seal.toml"),
            String::from("perish/wire/1.0.0/stable/source.tar"),
        ]
    );
    verify(&server, "perish/codec/1.0.0/stable");
    inspect(&server, &sandbox, &client);
}

fn verify(server: &Server, prefix: &str) {
    let source = format!("{prefix}/source.tar");
    let seal = format!("{prefix}/seal.toml");
    let writes: Vec<_> = server
        .events()
        .into_iter()
        .filter(|event| event.method == "PUT" && event.key.starts_with(prefix))
        .collect();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].key, source);
    assert_eq!(writes[1].key, seal);
    for item in writes {
        assert_eq!(item.guard.as_deref(), Some("*"));
        assert!(item.signed);
        assert_eq!(item.status, 200);
    }
}

fn inspect(server: &Server, sandbox: &Sandbox, client: &Client) {
    let wire = server
        .object("perish/wire/1.0.0/stable/source.tar")
        .expect("wire tar");
    let packed = server
        .object("perish/codec/1.0.0/stable/source.tar")
        .expect("codec tar");
    let raw = server
        .object("perish/codec/1.0.0/stable/seal.toml")
        .expect("codec seal");
    let entries = archive::unpack(&packed);
    assert_eq!(entries[0].path, "simplex.toml");
    for item in &entries {
        assert_eq!(item.modified, 0);
        assert_eq!(item.owner, 0);
        assert_eq!(item.group, 0);
    }
    let paths: Vec<&str> = entries.iter().map(|item| item.path.as_str()).collect();
    assert_eq!(paths, vec!["simplex.toml", "assets/table.bin", "codec.sx"]);
    let rewritten = String::from_utf8(entries[0].bytes.clone()).expect("manifest utf8");
    assert!(!rewritten.contains("workspace"));
    assert!(rewritten.contains("scope = \"perish\""));
    assert!(rewritten.contains("name = \"wire\""));
    assert!(rewritten.contains("version = \"1.0.0\""));
    assert!(rewritten.contains("channel = \"stable\""));

    let text = String::from_utf8(raw).expect("seal utf8");
    let stamp = archive::parse(&text);
    assert_eq!(stamp.schema, "simplex.seal/v1");
    assert_eq!(stamp.scope, "perish");
    assert_eq!(stamp.name, "codec");
    assert_eq!(stamp.version, "1.0.0");
    assert_eq!(stamp.channel, "stable");
    assert_eq!(stamp.source, crypto::render(&crypto::digest(&packed)));
    assert_eq!(stamp.dependencies.len(), 1);
    let dependency = &stamp.dependencies[0];
    assert_eq!(dependency.get("alias").map(String::as_str), Some("wire"));
    assert_eq!(dependency.get("scope").map(String::as_str), Some("perish"));
    assert_eq!(dependency.get("name").map(String::as_str), Some("wire"));
    assert_eq!(dependency.get("version").map(String::as_str), Some("1.0.0"));
    assert_eq!(
        dependency.get("channel").map(String::as_str),
        Some("stable")
    );
    assert_eq!(
        dependency.get("source").map(String::as_str),
        Some(crypto::render(&crypto::digest(&wire)).as_str())
    );

    let local = sandbox.join("sealed.toml");
    let sealed = sandbox.custom(
        &[
            "seal",
            "workspace/codec",
            "-o",
            local.to_str().expect("seal path"),
            "--endpoint",
            &client.endpoint,
            "--bucket",
            &client.bucket,
            "--region",
            &client.region,
        ],
        &[
            ("SIMPLEX_S3_ACCESS_KEY", "simplexkey"),
            ("SIMPLEX_S3_SECRET_KEY", "simplexsecret"),
        ],
    );
    assert!(sealed.passed, "seal cli failed: {}", sealed.stderr);
    assert_eq!(fs::read_to_string(&local).expect("read seal"), text);
}

#[test]
fn immutability() {
    let server = Server::tokenized("vault", "us-east-1", "ephemeraltoken");
    let sandbox = Sandbox::open();
    sandbox.write(
        "wire/simplex.toml",
        "\
schema = \"simplex.package/v1\"

[library]
scope = \"perish\"
name = \"wire\"
version = \"1.0.0\"
channel = \"stable\"
entry = \"wire.sx\"
",
    );
    sandbox.write("wire/wire.sx", "pub unit := () -> u64 { 7 }\n");

    let denied = sandbox.custom(
        &["publish", "wire"],
        &[
            ("SIMPLEX_S3_ENDPOINT", server.endpoint()),
            ("SIMPLEX_S3_BUCKET", server.bucket()),
            ("SIMPLEX_S3_REGION", "us-east-1"),
            ("SIMPLEX_S3_ACCESS_KEY", "simplexkey"),
            ("SIMPLEX_S3_SECRET_KEY", "wrongsecret"),
            ("SIMPLEX_S3_SESSION_TOKEN", "ephemeraltoken"),
        ],
    );
    assert!(!denied.passed);
    assert!(server.keys().is_empty());

    let valid = [
        ("SIMPLEX_S3_ENDPOINT", server.endpoint()),
        ("SIMPLEX_S3_BUCKET", server.bucket()),
        ("SIMPLEX_S3_REGION", "us-east-1"),
        ("SIMPLEX_S3_ACCESS_KEY", "simplexkey"),
        ("SIMPLEX_S3_SECRET_KEY", "simplexsecret"),
        ("SIMPLEX_S3_SESSION_TOKEN", "ephemeraltoken"),
    ];
    let published = sandbox.custom(&["publish", "wire"], &valid);
    assert!(
        published.passed,
        "tokenized publish failed: {}",
        published.stderr
    );

    let duplicate = sandbox.custom(&["publish", "wire"], &valid);
    assert!(!duplicate.passed);

    sandbox.write(
        "race/simplex.toml",
        "\
schema = \"simplex.package/v1\"

[library]
scope = \"perish\"
name = \"race\"
version = \"1.0.0\"
channel = \"stable\"
entry = \"race.sx\"
",
    );
    sandbox.write("race/race.sx", "pub ping := () -> u64 { 1 }\n");
    server.seed("perish/race/1.0.0/stable/source.tar", b"occupied".to_vec());

    let collided = sandbox.custom(&["publish", "race"], &valid);
    assert!(!collided.passed, "412 collision must fail closed");
    assert_eq!(
        server
            .object("perish/race/1.0.0/stable/source.tar")
            .as_deref(),
        Some(b"occupied".as_slice())
    );
    assert!(
        server
            .object("perish/race/1.0.0/stable/seal.toml")
            .is_none()
    );
    assert!(server.events().contains(&Event {
        method: String::from("PUT"),
        key: String::from("perish/race/1.0.0/stable/source.tar"),
        guard: Some(String::from("*")),
        signed: true,
        status: 412,
    }));
}

#[test]
fn refusals() {
    let server = Server::start("strict", "auto", false);
    let client = Client::signed(&server, "auto");
    let sandbox = Sandbox::open();
    harness::stage(&sandbox);
    sandbox.write("workspace/simplex.lock", "forbidden = true\n");
    assert!(
        !sandbox
            .run(&["publish", "workspace/wire"], Some(&client))
            .passed
    );
    assert!(
        !sandbox
            .run(&["seal", "workspace/wire"], Some(&client))
            .passed
    );
    fs::remove_file(sandbox.join("workspace/simplex.lock")).expect("remove workspace lock");

    sandbox.write("workspace/wire/simplex.lock", "forbidden = true\n");
    assert!(
        !sandbox
            .run(&["publish", "workspace/wire"], Some(&client))
            .passed
    );
    assert!(
        !sandbox
            .run(&["build", "workspace/wire"], Some(&client))
            .passed
    );
    fs::remove_file(sandbox.join("workspace/wire/simplex.lock")).expect("remove package lock");

    for broken in [
        "\
schema = \"simplex.package/v1\"
[library]
scope = \"perish\"
name = \"bad\"
version = \"1.0.0\"
channel = \"stable\"
[dependencies]
wire = { scope = \"perish\", name = \"wire\", version = \"^1.0.0\", channel = \"stable\" }
",
        "\
schema = \"simplex.package/v1\"
[library]
scope = \"perish\"
name = \"bad\"
version = \"1.0.0\"
channel = \"stable\"
[dependencies]
wire = { scope = \"perish\", name = \"wire\", version = \"1.0.0\" }
",
        "\
schema = \"simplex.package/v1\"
[library]
scope = \"perish\"
name = \"bad\"
version = \"1.0.0\"
",
        "\
schema = \"simplex.package/v1\"
[library]
scope = \"perish\"
name = \"bad\"
version = \"1.0.0-rc.1\"
channel = \"stable\"
",
    ] {
        sandbox.write("bad/simplex.toml", broken);
        sandbox.write("bad/bad.sx", "pub main := () -> i64 { 0 }\n");
        assert!(!sandbox.run(&["seal", "bad"], Some(&client)).passed);
        assert!(!sandbox.run(&["publish", "bad"], Some(&client)).passed);
    }
}
