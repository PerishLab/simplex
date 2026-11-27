use crate::archive;
use crate::crypto;
use crate::fixture;
use crate::harness::{Client, Sandbox};
use crate::server::{Event, Server};
use std::fs;

#[test]
fn rewrite() {
    let server = Server::start("simplex", "auto", false);
    let client = Client::signed(&server, "auto");
    let sandbox = Sandbox::open();
    fixture::stage(&sandbox);

    for early in ["workspace/lemmas/codec/json", "workspace/lemmas/relay"] {
        let premature = sandbox.run(&["publish", early], Some(&client));
        assert!(!premature.passed, "{early} published before wire");
    }
    assert!(server.keys().is_empty());

    fixture::publish(&sandbox, &client, &["workspace/lemmas/wire"]);
    verify(&server, "perish/wire/1.0.0/stable");
    let premature = sandbox.run(&["publish", "workspace/lemmas/relay"], Some(&client));
    assert!(!premature.passed, "relay published before codec.json");

    fixture::publish(&sandbox, &client, &["workspace/lemmas/codec/json"]);
    verify(&server, "perish/codec.json/1.0.0/stable");
    fixture::publish(&sandbox, &client, &["workspace/lemmas/relay"]);
    verify(&server, "perish/relay/1.0.0/stable");
    assert_eq!(
        server.keys(),
        vec![
            String::from("perish/codec.json/1.0.0/stable/seal.toml"),
            String::from("perish/codec.json/1.0.0/stable/source.tar"),
            String::from("perish/relay/1.0.0/stable/seal.toml"),
            String::from("perish/relay/1.0.0/stable/source.tar"),
            String::from("perish/wire/1.0.0/stable/seal.toml"),
            String::from("perish/wire/1.0.0/stable/source.tar"),
        ]
    );
    layout(&server);
    inspect(&server, &sandbox, &client);
}

fn verify(server: &Server, prefix: &str) {
    let writes: Vec<_> = server
        .events()
        .into_iter()
        .filter(|event| event.method == "PUT" && event.key.starts_with(&format!("{prefix}/")))
        .collect();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].key, format!("{prefix}/source.tar"));
    assert_eq!(writes[1].key, format!("{prefix}/seal.toml"));
    for item in writes {
        assert_eq!(item.guard.as_deref(), Some("*"));
        assert!(item.signed);
        assert_eq!(item.status, 200);
    }
}

fn layout(server: &Server) {
    let packed = server
        .object("perish/codec.json/1.0.0/stable/source.tar")
        .expect("codec.json tar");
    let entries = archive::unpack(&packed);
    let paths: Vec<&str> = entries.iter().map(|item| item.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "simplex.toml",
            "assets/table.bin",
            "format.sx",
            "internal.sx",
            "parse.sx"
        ]
    );
    for item in &entries {
        assert_eq!(item.modified, 0);
        assert_eq!(item.owner, 0);
        assert_eq!(item.group, 0);
    }
    let rewritten = String::from_utf8(entries[0].bytes.clone()).expect("manifest utf8");
    assert!(rewritten.contains("schema = \"simplex.lemma/v1\""));
    assert!(rewritten.contains("[lemma]"));
    assert!(rewritten.contains("name = \"codec.json\""));
    assert!(!rewritten.contains("workspace"));
    assert!(!rewritten.contains("entry"));
    assert!(!rewritten.contains("[library]"));
}

fn inspect(server: &Server, sandbox: &Sandbox, client: &Client) {
    let packed = server
        .object("perish/relay/1.0.0/stable/source.tar")
        .expect("relay tar");
    let entries = archive::unpack(&packed);
    let paths: Vec<&str> = entries.iter().map(|item| item.path.as_str()).collect();
    assert_eq!(paths, vec!["simplex.toml", "bridge.sx"]);
    let rewritten = String::from_utf8(entries[0].bytes.clone()).expect("manifest utf8");
    assert!(!rewritten.contains("workspace"));
    assert!(rewritten.contains("name = \"wire\""));
    assert!(rewritten.contains("name = \"codec.json\""));
    assert!(rewritten.contains("version = \"1.0.0\""));
    assert!(rewritten.contains("channel = \"stable\""));

    let raw = server
        .object("perish/relay/1.0.0/stable/seal.toml")
        .expect("relay seal");
    let text = String::from_utf8(raw).expect("seal utf8");
    let stamp = archive::parse(&text);
    assert_eq!(stamp.schema, "simplex.seal/v1");
    assert_eq!(stamp.name, "relay");
    assert_eq!(stamp.source, crypto::render(&crypto::digest(&packed)));
    assert_eq!(stamp.dependencies.len(), 2);
    for (alias, name) in [("json", "codec.json"), ("wire", "wire")] {
        let found = stamp
            .dependencies
            .iter()
            .find(|item| item.get("alias").map(String::as_str) == Some(alias))
            .expect("locked dependency");
        assert_eq!(found.get("scope").map(String::as_str), Some("perish"));
        assert_eq!(found.get("name").map(String::as_str), Some(name));
        assert_eq!(found.get("version").map(String::as_str), Some("1.0.0"));
        assert_eq!(found.get("channel").map(String::as_str), Some("stable"));
        let key = format!("perish/{name}/1.0.0/stable/source.tar");
        let bytes = server.object(&key).expect("dependency tar");
        let digest = crypto::render(&crypto::digest(&bytes));
        assert_eq!(
            found.get("source").map(String::as_str),
            Some(digest.as_str())
        );
    }

    let local = sandbox.join("sealed.toml");
    let sealed = sandbox.custom(
        &[
            "seal",
            "workspace/lemmas/relay",
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
        &fixture::manifest("wire", "1.0.0", "stable", ""),
    );
    sandbox.write("wire/unit.sx", "pub unit := () -> u64 { 7 }\n");
    let mut variables = vec![
        ("SIMPLEX_S3_ENDPOINT", server.endpoint()),
        ("SIMPLEX_S3_BUCKET", server.bucket()),
        ("SIMPLEX_S3_REGION", "us-east-1"),
        ("SIMPLEX_S3_ACCESS_KEY", "simplexkey"),
        ("SIMPLEX_S3_SECRET_KEY", "wrongsecret"),
        ("SIMPLEX_S3_SESSION_TOKEN", "ephemeraltoken"),
    ];
    assert!(!sandbox.custom(&["publish", "wire"], &variables).passed);
    assert!(server.keys().is_empty());

    variables[4] = ("SIMPLEX_S3_SECRET_KEY", "simplexsecret");
    let published = sandbox.custom(&["publish", "wire"], &variables);
    assert!(
        published.passed,
        "tokenized publish failed: {}",
        published.stderr
    );
    assert!(!sandbox.custom(&["publish", "wire"], &variables).passed);

    sandbox.write(
        "race/simplex.toml",
        &fixture::manifest("race", "1.0.0", "stable", ""),
    );
    sandbox.write("race/ping.sx", "pub ping := () -> u64 { 1 }\n");
    server.seed("perish/race/1.0.0/stable/source.tar", b"occupied".to_vec());
    let collided = sandbox.custom(&["publish", "race"], &variables);
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
    fixture::stage(&sandbox);
    let wire = "workspace/lemmas/wire";
    let library = sandbox.join("wire.so");
    let output = library.to_str().expect("library path");
    let verbs: [&[&str]; 3] = [
        &["publish", wire],
        &["seal", wire],
        &["build", wire, "--dylib", "-o", output],
    ];
    for lock in [
        "workspace/simplex.lock",
        "workspace/lemmas/wire/simplex.lock",
    ] {
        sandbox.write(lock, "forbidden = true\n");
        for verb in verbs {
            assert!(
                !sandbox.run(verb, Some(&client)).passed,
                "{verb:?} accepted {lock}"
            );
        }
        fs::remove_file(sandbox.join(lock)).expect("remove lock");
    }
    assert!(server.keys().is_empty());
    assert!(!library.exists());
    fixture::publish(&sandbox, &client, &[wire, "workspace/lemmas/codec/json"]);

    let relay = "workspace/lemmas/relay";
    for target in ["^1.0", "1.0.0", "", "codec.json.stream", "Codec", "*.json"] {
        let line =
            format!("wire = {{ workspace = \"*\" }}\njson = {{ workspace = \"{target}\" }}\n");
        sandbox.write(
            "workspace/lemmas/relay/simplex.toml",
            &fixture::manifest("relay", "1.0.0", "stable", &line),
        );
        for verb in ["seal", "publish"] {
            let outcome = sandbox.run(&[verb, relay], Some(&client));
            assert!(
                !outcome.passed,
                "{verb} accepted workspace target {target:?}"
            );
        }
    }
    let line = "wire = { workspace = \"wire\" }\njson = { workspace = \"codec.json\" }\n";
    sandbox.write(
        "workspace/lemmas/relay/simplex.toml",
        &fixture::manifest("relay", "1.0.0", "stable", line),
    );
    let control = sandbox.run(&["seal", relay], Some(&client));
    assert!(
        control.passed,
        "explicit workspace targets refused: {}",
        control.stderr
    );
    assert!(
        server
            .object("perish/relay/1.0.0/stable/seal.toml")
            .is_none()
    );
}
