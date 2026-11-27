use super::support::{check, node, write};
use simplex_hir::{Graph, build};
use simplex_ir::Linkage;
use simplex_package::Coordinate;
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn dynamicgraph() {
    let scratch = tempdir().expect("tempdir");
    let wiredir = scratch.path().join("wire");
    let codecdir = scratch.path().join("codec");
    let pipedir = scratch.path().join("pipe");
    let appdir = scratch.path().join("app");
    write(
        &wiredir,
        "wire.sx",
        concat!(
            "pub dylib Frame := struct {\n    pub head: u64\n    pub body: u64\n}\n\n",
            "pub internal := (frame: &Frame) -> u64 {\n    frame.head\n}\n\n",
            "pub dylib checksum := (frame: &Frame) -> u64 {\n    frame.head * 3 + frame.body\n}\n",
        ),
    );
    write(
        &codecdir,
        "codec.sx",
        concat!(
            "wire := dylib wire\n\n",
            "pub dylib Cipher := interface {\n    mix: (self: &Self, seed: u64) -> u64\n}\n\n",
            "pub dylib encode := (cipher: &Cipher, head: u64, body: u64) -> u64 {\n",
            "    var frame := wire.Frame { head: head, body: body }\n",
            "    cipher.mix(wire.checksum(&frame))\n",
            "}\n",
        ),
    );
    write(
        &pipedir,
        "pipe.sx",
        concat!(
            "wire := dylib wire\n\n",
            "pub wrap := (value: u64) -> u64 {\n",
            "    var frame := wire.Frame { head: value, body: 5 }\n",
            "    wire.checksum(&frame)\n",
            "}\n",
        ),
    );
    write(
        &appdir,
        "app.sx",
        concat!(
            "codec := dylib codec\n",
            "pipe := import pipe\n\n",
            "Rotator := struct {\n    shift: u64\n}\n\n",
            "Rotator : codec.Cipher {\n",
            "    mix: (self: &Rotator, seed: u64) -> u64 {\n        seed + self.shift\n    }\n",
            "}\n\n",
            "pub main := () -> i64 {\n",
            "    var rot := Rotator { shift: 9 }\n",
            "    (codec.encode(&rot, 4, 7) + pipe.wrap(2)).as<i64>()\n",
            "}\n",
        ),
    );
    let mut packages = BTreeMap::new();
    for item in [
        node("perish/wire/1.0.0/canary", &wiredir, "wire.sx", &[]),
        node(
            "perish/codec/2.1.0/stable",
            &codecdir,
            "codec.sx",
            &[("wire", "perish/wire/1.0.0/canary")],
        ),
        node(
            "perish/pipe/1.0.0/stable",
            &pipedir,
            "pipe.sx",
            &[("wire", "perish/wire/1.0.0/canary")],
        ),
    ] {
        packages.insert(item.coordinate.clone(), item);
    }
    let root = node(
        "perish/app/1.0.0/stable",
        &appdir,
        "app.sx",
        &[
            ("codec", "perish/codec/2.1.0/stable"),
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
    assert_eq!(plan.namespace, "perish$app$1_0_0$stable");
    assert_eq!(plan.stem, "app");
    assert_eq!(plan.dynamics.len(), 2);
    assert_eq!(
        plan.dynamics[0].0,
        Coordinate::parse("perish/wire/1.0.0/canary").unwrap()
    );
    assert_eq!(
        plan.dynamics[1].0,
        Coordinate::parse("perish/codec/2.1.0/stable").unwrap()
    );
    assert!(plan.module.externals.iter().any(|ext| {
        ext.symbol == "sx$perish$codec$2_1_0$stable$codec$encode"
            && ext.linkage == Linkage::Dynamic(String::from("sx_perish_codec_2_1_0_stable"))
    }));
    assert!(plan.module.externals.iter().any(|ext| {
        ext.symbol == "sx$perish$wire$1_0_0$canary$wire$checksum"
            && ext.linkage == Linkage::Dynamic(String::from("sx_perish_wire_1_0_0_canary"))
    }));
    assert!(plan.dynamics[0].1.functions.iter().any(|f| {
        f.symbol == "sx$perish$wire$1_0_0$canary$wire$checksum" && f.linkage == Linkage::Exported
    }));
    assert!(plan.dynamics[0].1.functions.iter().any(|f| {
        f.symbol == "sx$perish$wire$1_0_0$canary$wire$internal" && f.linkage == Linkage::Internal
    }));
    check(&plan.stem, &plan.module, false);
    for (coord, dynmod) in &plan.dynamics {
        check(&coord.stem(), dynmod, true);
    }
    let sharedroot = packages
        .get(&Coordinate::parse("perish/wire/1.0.0/canary").unwrap())
        .unwrap()
        .clone();
    let sharedplan = build(
        &Graph {
            root: sharedroot,
            packages: BTreeMap::new(),
        },
        true,
    )
    .expect("shared root");
    assert_eq!(sharedplan.stem, "sx_perish_wire_1_0_0_canary");
    check(&sharedplan.stem, &sharedplan.module, true);
}
