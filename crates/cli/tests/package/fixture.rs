use crate::crypto;
use crate::harness::{Client, Sandbox};
use std::fs;
use std::path::Path;

pub const PACKET: &str = "\
pub Packet := struct {
    pub tag: u8
    pub base: u64
}
pub lift := (packet: &Packet) -> u64 {
    packet.tag.into<u64>() + packet.base
}
";

pub const PARSE: &str = "\
packet := import wire.packet
table := embed(\"assets/table.bin\")
pub scan := (seed: u64) -> u64 {
    frame := packet.Packet { tag: table[1], base: seed + table[3].into<u64>() }
    packet.lift(&frame) + frame.tag.into<u64>() + table.len().into<u64>() + table[0].into<u64>()
}
";

pub const INTERNAL: &str = "\
seed := () -> u64 {
    3
}
spread := (value: u64) -> u64 {
    value + seed()
}
";

pub const BRIDGE: &str = "\
packet := import wire.packet
parse := import json.parse
pub relay := (seed: u64) -> u64 {
    frame := packet.Packet { tag: 1, base: seed }
    packet.lift(&frame) + frame.base + parse.scan(seed)
}
";

pub fn manifest(name: &str, version: &str, channel: &str, dependencies: &str) -> String {
    format!(
        "\
schema = \"simplex.lemma/v1\"

[lemma]
scope = \"perish\"
name = \"{name}\"
version = \"{version}\"
channel = \"{channel}\"

[dependencies]
{dependencies}"
    )
}

pub fn exact(alias: &str, name: &str, channel: &str) -> String {
    format!(
        "{alias} = {{ scope = \"perish\", name = \"{name}\", version = \"1.0.0\", channel = \"{channel}\" }}\n"
    )
}

pub fn stem(name: &str, version: &str, channel: &str) -> String {
    let mut bytes = b"simplex.dylib/v1\0".to_vec();
    bytes.extend_from_slice(format!("perish/{name}/{version}/{channel}").as_bytes());
    let hex = crypto::render(&crypto::digest(&bytes));
    hex[..16].to_owned()
}

pub fn metric(bonus: u64) -> String {
    format!(
        "\
pub dylib Pair := struct {{
    pub left: u64
    pub right: u64
}}
pub dylib Metric := interface {{
    weigh: (self: &Self, pair: &Pair) -> u64
}}
pub dylib combine := (metric: &Metric, pair: &Pair) -> u64 {{
    metric.weigh(pair) + pair.left + pair.right + {bonus}
}}
"
    )
}

pub fn stage(sandbox: &Sandbox) {
    sandbox.write(
        "workspace/simplex.toml",
        "schema = \"simplex.workspace/v1\"\nmembers = [\"lemmas/wire\", \"lemmas/codec/json\", \"lemmas/relay\"]\n",
    );
    sandbox.write(
        "workspace/lemmas/wire/simplex.toml",
        &manifest("wire", "1.0.0", "stable", ""),
    );
    sandbox.write("workspace/lemmas/wire/packet.sx", PACKET);
    sandbox.write(
        "workspace/lemmas/codec/json/simplex.toml",
        &manifest(
            "codec.json",
            "1.0.0",
            "stable",
            "wire = { workspace = \"*\" }\n",
        ),
    );
    sandbox.asset(
        "workspace/lemmas/codec/json/assets/table.bin",
        &[4, 7, 9, 13],
    );
    sandbox.write("workspace/lemmas/codec/json/parse.sx", PARSE);
    sandbox.write("workspace/lemmas/codec/json/format.sx", &metric(5));
    sandbox.write("workspace/lemmas/codec/json/internal.sx", INTERNAL);
    sandbox.write(
        "workspace/lemmas/relay/simplex.toml",
        &manifest(
            "relay",
            "1.0.0",
            "stable",
            "wire = { workspace = \"*\" }\njson = { workspace = \"codec.json\" }\n",
        ),
    );
    sandbox.write("workspace/lemmas/relay/bridge.sx", BRIDGE);
}

pub fn math(sandbox: &Sandbox, folder: &str, channel: &str, bonus: u64) {
    sandbox.write(
        &format!("{folder}/simplex.toml"),
        &manifest("math", "1.0.0", channel, ""),
    );
    sandbox.write(&format!("{folder}/metric.sx"), &metric(bonus));
}

pub fn publish(sandbox: &Sandbox, client: &Client, targets: &[&str]) {
    for target in targets {
        let outcome = sandbox.run(&["publish", target], Some(client));
        assert!(
            outcome.passed,
            "publish {target} failed: {}",
            outcome.stderr
        );
    }
}

pub fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

pub fn listing(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .expect("read output folder")
        .map(|item| {
            item.expect("folder entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    for name in &names {
        assert!(!name.starts_with("lib"), "legacy lib prefix {name}");
        assert!(!name.starts_with("sx_"), "legacy sx_ prefix {name}");
    }
    names
}
