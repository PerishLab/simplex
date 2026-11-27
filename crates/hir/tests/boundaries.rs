use simplex_hir::{Expression, Statement, Type, bridge, lower};
use simplex_ir::Linkage;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn modules() {
    let path = Path::new("../../cases/module/pass/packet.sx");
    let unit = lower("app", path, false).expect("lower packet");
    assert!(unit.catalog.records.contains_key("Span"));
    assert!(unit.catalog.records.contains_key("Packet"));
    assert!(
        unit.routines
            .iter()
            .any(|r| r.symbol == "sx$app$span$end" && r.exported)
    );
    assert!(
        unit.routines
            .iter()
            .any(|r| r.symbol == "sx$app$packet$make" && r.exported)
    );
    assert!(
        unit.routines
            .iter()
            .any(|r| r.symbol == "sx$app$packet$secret" && !r.exported)
    );
}

#[test]
fn shared() {
    let codec = Path::new("../../cases/dylib/pass/codec.sx");
    let lib = lower("app", codec, true).expect("lower codec dylib");
    assert!(lib.shared);
    assert_eq!(lib.entry, None);
    assert!(
        lib.routines
            .iter()
            .any(|r| r.symbol == "sx$app$codec$transcode" && r.exported)
    );
    assert!(
        lib.routines
            .iter()
            .any(|r| r.symbol == "sx$app$codec$helper" && !r.exported)
    );
    let transcode = lib
        .routines
        .iter()
        .find(|r| r.symbol == "sx$app$codec$transcode")
        .expect("transcode");
    assert_eq!(transcode.inputs[0].1, Type::Trait(String::from("Codec")));
    assert!(transcode.body.iter().any(|s| matches!(
        s,
        Statement::Bind {
            value: Expression::Virtual { slot: 0, .. },
            ..
        }
    )));
}

#[test]
fn consumer() {
    let plugin = Path::new("../../cases/dylib/pass/plugin.sx");
    let unit = lower("app", plugin, false).expect("lower plugin");
    assert_eq!(unit.libraries, vec![String::from("codec.so")]);
    assert!(unit.imports.iter().any(|i| {
        i.symbol == "sx$app$codec$transcode" && i.library.as_deref() == Some("codec.so")
    }));
    assert!(unit.tables.iter().any(|t| {
        t.symbol == "sx$vt$app$plugin$Xor$Codec"
            && t.methods == vec![String::from("sx$impl$app$plugin$Xor$Codec$encode")]
    }));
}

#[test]
fn foreign() {
    let ffi = Path::new("../../cases/module/pass/ffi.sx");
    let unit = lower("app", ffi, false).expect("lower ffi");
    assert_eq!(unit.libraries, vec![String::from("libc.so.6")]);
    let imp = unit
        .imports
        .iter()
        .find(|i| i.symbol == "epoll_wait")
        .expect("epoll_wait import");
    assert_eq!(imp.library.as_deref(), Some("libc.so.6"));
    assert_eq!(imp.output, Type::Signed(4));
}

#[test]
fn zeroprefix() {
    let scratch = tempdir().expect("tempdir");
    fs::write(
        scratch.path().join("libwrap.sx"),
        "pub dylib ping := (x: i64) -> i64 {\n    x + 1\n}\n",
    )
    .expect("write libwrap");
    fs::write(
        scratch.path().join("app.sx"),
        "libwrap := dylib libwrap\n\npub main := () -> i64 {\n    libwrap.ping(2)\n}\n",
    )
    .expect("write app");
    let unit = lower("app", &scratch.path().join("app.sx"), false).expect("lower app");
    assert_eq!(unit.libraries, vec![String::from("libwrap.so")]);
    let module = bridge(&unit);
    assert!(module.externals.iter().any(|ext| {
        ext.symbol == "sx$app$libwrap$ping"
            && ext.linkage == Linkage::Dynamic(String::from("libwrap"))
    }));
}
