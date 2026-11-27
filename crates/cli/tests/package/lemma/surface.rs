use crate::archive;
use crate::fixture;
use crate::harness::{Client, Sandbox};
use crate::server::Server;
use std::fs;

const PARSE: &str = "\
table := embed(\"assets/table.bin\")
pub Token := struct {
    pub kind: u8
    pub size: u64
}
pub scan := (token: &Token) -> u64 {
    token.kind.into<u64>() + token.size + table.len().into<u64>()
}
";

const STREAM: &str = "\
pub drain := (value: u64) -> u64 {
    value * 3
}
";

fn stamp(server: &Server, version: &str) -> archive::Stamp {
    let key = format!("perish/codec.json/{version}/stable/seal.toml");
    let bytes = server.object(&key).expect("published seal");
    archive::parse(&String::from_utf8(bytes).expect("seal utf8"))
}

fn release(sandbox: &Sandbox, version: &str) {
    sandbox.write(
        "json/simplex.toml",
        &fixture::manifest("codec.json", version, "stable", ""),
    );
}

fn refused(sandbox: &Sandbox, client: &Client, server: &Server) {
    assert!(!sandbox.run(&["publish", "json"], Some(client)).passed);
    let baseline = [
        "seal",
        "json",
        "--baseline",
        "perish/codec.json/1.0.1/stable",
    ];
    assert!(!sandbox.run(&baseline, Some(client)).passed);
    assert!(
        server
            .object("perish/codec.json/1.0.2/stable/seal.toml")
            .is_none()
    );
}

#[test]
fn evolution() {
    let server = Server::start("surface", "auto", false);
    let client = Client::signed(&server, "auto");
    let sandbox = Sandbox::open();
    release(&sandbox, "1.0.0");
    sandbox.asset("json/assets/table.bin", &[4, 7, 9, 13]);
    sandbox.write("json/parse.sx", PARSE);
    sandbox.write("json/format.sx", &fixture::metric(5));
    sandbox.write("json/internal.sx", fixture::INTERNAL);
    fixture::publish(&sandbox, &client, &["json"]);
    let first = stamp(&server, "1.0.0");

    release(&sandbox, "1.0.1");
    sandbox.write("json/stream.sx", STREAM);
    sandbox.write("json/internal.sx", &fixture::INTERNAL.replace("3", "11"));
    fixture::publish(&sandbox, &client, &["json"]);
    let second = stamp(&server, "1.0.1");
    assert_ne!(first.source, second.source);
    assert_ne!(first.api, second.api);
    let packed = server
        .object("perish/codec.json/1.0.1/stable/source.tar")
        .expect("1.0.1 tar");
    let paths: Vec<String> = archive::unpack(&packed)
        .into_iter()
        .map(|item| item.path)
        .collect();
    let expected = [
        "simplex.toml",
        "assets/table.bin",
        "format.sx",
        "internal.sx",
        "parse.sx",
        "stream.sx",
    ];
    assert_eq!(paths, expected);

    release(&sandbox, "1.0.2");
    let broken = PARSE
        .replace("(token: &Token)", "(token: &Token, factor: u64)")
        .replace(
            "table.len().into<u64>()",
            "table.len().into<u64>() * factor",
        );
    sandbox.write("json/parse.sx", &broken);
    refused(&sandbox, &client, &server);
    fs::remove_file(sandbox.join("json/parse.sx")).expect("remove parse");
    refused(&sandbox, &client, &server);
    sandbox.write("json/parse.sx", PARSE);
    sandbox.write(
        "json/format.sx",
        &fixture::metric(5).replace(
            "    pub left: u64\n    pub right: u64\n",
            "    pub right: u64\n    pub left: u64\n",
        ),
    );
    refused(&sandbox, &client, &server);
    sandbox.write("json/format.sx", &fixture::metric(5));
    fs::remove_file(sandbox.join("json/stream.sx")).expect("remove stream");
    refused(&sandbox, &client, &server);

    release(&sandbox, "1.1.0");
    sandbox.write("json/stream.sx", STREAM);
    sandbox.write("json/parse.sx", &broken);
    fixture::publish(&sandbox, &client, &["json"]);
    let third = stamp(&server, "1.1.0");
    assert_ne!(second.api, third.api);
    assert!(
        server
            .object("perish/codec.json/1.0.2/stable/seal.toml")
            .is_none()
    );
}
