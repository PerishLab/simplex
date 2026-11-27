use super::support::{check, node, write};
use simplex_hir::{Graph, Mangle, build};
use simplex_ir::Linkage;
use simplex_package::Coordinate;
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn dynamicgraph() {
    let scratch = tempdir().expect("tempdir");
    let wiredir = scratch.path().join("wire");
    let jsondir = scratch.path().join("codec.json");
    let pipedir = scratch.path().join("pipe");
    let appdir = scratch.path().join("app");
    write(
        &wiredir,
        "frame.sx",
        concat!(
            "pub dylib Frame := struct {\n    pub head: u64\n    pub body: u64\n}\n\n",
            "pub internal := (frame: &Frame) -> u64 {\n    frame.head\n}\n\n",
            "pub dylib checksum := (frame: &Frame) -> u64 {\n    frame.head * 3 + frame.body\n}\n",
        ),
    );
    write(
        &jsondir,
        "parse.sx",
        concat!(
            "frame := dylib wire.frame\n\n",
            "pub dylib parse := (head: u64, body: u64) -> u64 {\n",
            "    var item := frame.Frame { head: head, body: body }\n",
            "    frame.checksum(&item)\n",
            "}\n",
        ),
    );
    write(
        &jsondir,
        "format.sx",
        concat!(
            "frame := dylib wire.frame\n\n",
            "pub dylib Cipher := interface {\n    mix: (self: &Self, seed: u64) -> u64\n}\n\n",
            "pub dylib encode := (cipher: &Cipher, head: u64, body: u64) -> u64 {\n",
            "    var item := frame.Frame { head: head, body: body }\n",
            "    cipher.mix(frame.checksum(&item))\n",
            "}\n",
        ),
    );
    write(
        &jsondir,
        "lexer/tok.sx",
        "pub dylib tag := (value: u64) -> u64 {\n    value + 10\n}\n",
    );
    std::fs::create_dir_all(jsondir.join(".hidden")).expect("mkdir .hidden");
    std::fs::write(jsondir.join(".hidden/ignored.sx"), "bad syntax :::").expect("hidden");
    std::fs::create_dir_all(jsondir.join("target")).expect("mkdir target");
    std::fs::write(jsondir.join("target/ignored.sx"), "bad syntax :::").expect("target");
    std::fs::create_dir_all(jsondir.join("nested")).expect("mkdir nested");
    std::fs::write(jsondir.join("nested/simplex.toml"), "").expect("nested manifest");
    std::fs::write(jsondir.join("nested/ignored.sx"), "bad syntax :::").expect("nested");
    write(
        &pipedir,
        "wrap.sx",
        concat!(
            "frame := dylib wire.frame\n\n",
            "pub wrap := (value: u64) -> u64 {\n",
            "    var item := frame.Frame { head: value, body: 5 }\n",
            "    frame.checksum(&item)\n",
            "}\n",
        ),
    );
    write(
        &appdir,
        "main.sx",
        concat!(
            "parse := dylib json.parse\n",
            "format := dylib json.format\n",
            "tok := dylib json.lexer.tok\n",
            "wrap := import pipe.wrap\n\n",
            "Rotator := struct {\n    shift: u64\n}\n\n",
            "Rotator : format.Cipher {\n",
            "    mix: (self: &Rotator, seed: u64) -> u64 {\n        seed + self.shift\n    }\n",
            "}\n\n",
            "pub main := () -> i64 {\n",
            "    var rot := Rotator { shift: 9 }\n",
            "    (format.encode(&rot, 4, 7) + parse.parse(2, 3) + tok.tag(1) + wrap.wrap(2)).as<i64>()\n",
            "}\n",
        ),
    );
    let wirecoord = Coordinate::parse("perish/wire/1.0.0/canary").unwrap();
    let jsoncoord = Coordinate::parse("perish/codec.json/2.1.0/stable").unwrap();
    let pipecoord = Coordinate::parse("perish/pipe/1.0.0/stable").unwrap();
    let appcoord = Coordinate::parse("perish/tool.cli/1.0.0/stable").unwrap();
    let mut packages = BTreeMap::new();
    for item in [
        node("perish/wire/1.0.0/canary", &wiredir, &[]),
        node(
            "perish/codec.json/2.1.0/stable",
            &jsondir,
            &[("wire", "perish/wire/1.0.0/canary")],
        ),
        node(
            "perish/pipe/1.0.0/stable",
            &pipedir,
            &[("wire", "perish/wire/1.0.0/canary")],
        ),
    ] {
        packages.insert(item.coordinate.clone(), item);
    }
    let root = node(
        "perish/tool.cli/1.0.0/stable",
        &appdir,
        &[
            ("json", "perish/codec.json/2.1.0/stable"),
            ("pipe", "perish/pipe/1.0.0/stable"),
        ],
    );
    let plan = build(
        &Graph {
            root,
            packages: packages.clone(),
        },
        false,
    )
    .expect("build dylib plan");
    assert_eq!(plan.namespace, appcoord.namespace());
    assert_eq!(plan.namespace.len(), 16);
    assert_eq!(plan.stem, "cli");
    assert_eq!(plan.dynamics.len(), 2);
    assert_eq!(plan.dynamics[0].0, wirecoord);
    assert_eq!(plan.dynamics[1].0, jsoncoord);
    assert_eq!(pipecoord.leaf(), "pipe");
    assert_eq!(jsoncoord.leaf(), "json");
    assert_eq!(jsoncoord.stem().len(), 16);
    let encodesym = Mangle::symbol(&jsoncoord.namespace(), "format", "encode");
    let parsesym = Mangle::symbol(&jsoncoord.namespace(), "parse", "parse");
    let tagsym = Mangle::symbol(&jsoncoord.namespace(), "lexer$tok", "tag");
    let wiresym = Mangle::symbol(&wirecoord.namespace(), "frame", "checksum");
    let internalsym = Mangle::symbol(&wirecoord.namespace(), "frame", "internal");
    for sym in [&encodesym, &parsesym, &tagsym] {
        assert!(plan.module.externals.iter().any(|ext| {
            ext.symbol == *sym && ext.linkage == Linkage::Dynamic(jsoncoord.stem())
        }));
    }
    assert!(
        plan.module.externals.iter().any(|ext| {
            ext.symbol == wiresym && ext.linkage == Linkage::Dynamic(wirecoord.stem())
        })
    );
    assert!(
        plan.dynamics[0]
            .1
            .functions
            .iter()
            .any(|f| f.symbol == wiresym && f.linkage == Linkage::Exported)
    );
    assert!(
        plan.dynamics[0]
            .1
            .functions
            .iter()
            .any(|f| f.symbol == internalsym && f.linkage == Linkage::Internal)
    );
    for sym in [&encodesym, &parsesym, &tagsym] {
        assert!(
            plan.dynamics[1]
                .1
                .functions
                .iter()
                .any(|f| f.symbol == *sym && f.linkage == Linkage::Exported)
        );
    }
    check(&plan.stem, &plan.module, false);
    for (coord, dynmod) in &plan.dynamics {
        check(&coord.stem(), dynmod, true);
    }
    let sharedroot = packages.get(&jsoncoord).unwrap().clone();
    let sharedplan = build(
        &Graph {
            root: sharedroot,
            packages,
        },
        true,
    )
    .expect("shared root");
    assert_eq!(sharedplan.stem, jsoncoord.stem());
    for sym in [&encodesym, &parsesym, &tagsym] {
        assert!(
            sharedplan
                .module
                .functions
                .iter()
                .any(|f| f.symbol == *sym && f.linkage == Linkage::Exported)
        );
    }
    check(&sharedplan.stem, &sharedplan.module, true);
}
