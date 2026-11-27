use super::{UNIT, attempt, refuse};
use crate::fixture;
use crate::harness::{Client, Sandbox};
use crate::server::Server;
use std::fs;

#[test]
fn naming() {
    let server = Server::start("names", "auto", false);
    let client = Client::signed(&server, "auto");
    let sandbox = Sandbox::open();
    for (folder, name) in [("one", "codec"), ("two", "codec.json")] {
        sandbox.write(
            &format!("{folder}/simplex.toml"),
            &fixture::manifest(name, "1.0.0", "stable", ""),
        );
        sandbox.write(&format!("{folder}/unit.sx"), UNIT);
        for outcome in attempt(&sandbox, &client, folder) {
            assert!(outcome.passed, "{name} refused: {}", outcome.stderr);
        }
    }
    let expected = vec![
        String::from("perish/codec.json/1.0.0/stable/seal.toml"),
        String::from("perish/codec.json/1.0.0/stable/source.tar"),
        String::from("perish/codec/1.0.0/stable/seal.toml"),
        String::from("perish/codec/1.0.0/stable/source.tar"),
    ];
    assert_eq!(fixture::sorted(server.keys()), expected);

    let names = [
        "codec.json.stream",
        ".json",
        "codec.",
        "codec_json",
        "codec..json",
        "..",
        "Codec",
        "codec-json",
        "codec/json",
        "",
    ];
    for name in names {
        sandbox.write(
            "bad/simplex.toml",
            &fixture::manifest(name, "1.0.0", "stable", ""),
        );
        sandbox.write("bad/unit.sx", UNIT);
        refuse(&sandbox, &client, "bad");
    }
    for scope in ["perish.lab", "perish_lab", "Perish", ""] {
        let text = fixture::manifest("codec", "1.0.0", "stable", "");
        let swapped = text.replace("scope = \"perish\"", &format!("scope = \"{scope}\""));
        sandbox.write("bad/simplex.toml", &swapped);
        refuse(&sandbox, &client, "bad");
    }
    assert_eq!(fixture::sorted(server.keys()), expected);
}

#[test]
fn schema() {
    let server = Server::start("schema", "auto", false);
    let client = Client::signed(&server, "auto");
    let sandbox = Sandbox::open();
    let valid = fixture::manifest("bad", "1.0.0", "stable", "");
    let broken = [
        valid.replace("simplex.lemma/v1", "simplex.package/v1"),
        valid.replace("[lemma]", "[library]"),
        valid.replace(
            "channel = \"stable\"\n",
            "channel = \"stable\"\nentry = \"unit.sx\"\n",
        ),
        valid
            .replace("simplex.lemma/v1", "simplex.package/v1")
            .replace("[lemma]", "[library]")
            .replace(
                "channel = \"stable\"\n",
                "channel = \"stable\"\nentry = \"unit.sx\"\n",
            ),
        valid.replace("channel = \"stable\"\n", ""),
        valid.replace("1.0.0", "1.0.0-rc.1"),
        valid.clone()
            + "wire = { scope = \"perish\", name = \"wire\", version = \"^1.0.0\", channel = \"stable\" }\n",
        valid.clone() + "wire = { scope = \"perish\", name = \"wire\", version = \"1.0.0\" }\n",
    ];
    sandbox.write("bad/unit.sx", UNIT);
    for text in broken {
        sandbox.write("bad/simplex.toml", &text);
        refuse(&sandbox, &client, "bad");
    }
    sandbox.write("bad/simplex.toml", &valid);
    sandbox.write("bad/simplex.lock", "forbidden = true\n");
    refuse(&sandbox, &client, "bad");
    assert!(server.keys().is_empty());

    fs::remove_file(sandbox.join("bad/simplex.lock")).expect("remove lock");
    for outcome in attempt(&sandbox, &client, "bad") {
        assert!(
            outcome.passed,
            "entry-free lemma refused: {}",
            outcome.stderr
        );
    }
    assert!(server.object("perish/bad/1.0.0/stable/seal.toml").is_some());
}
