use simplex_package::{
    Contract, Coordinate, Dependency, Fault, Manifest, Release, Seal, Verdict, Version, audit,
    surface,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn manifest(version: &str, deps: &[&str]) -> Manifest {
    let parsed = Version::parse(version).unwrap();
    let lemma = Coordinate::new("perish", "codec.json", parsed, "stable").unwrap();
    let mut dependencies = BTreeMap::new();
    for name in deps {
        let dep =
            Coordinate::new("perish", name, Version::parse("1.0.0").unwrap(), "stable").unwrap();
        dependencies.insert((*name).to_owned(), Dependency::Exact(dep));
    }
    Manifest {
        lemma,
        dependencies,
    }
}

fn stamp(manifest: &Manifest, contract: &Contract) -> Seal {
    Seal {
        coordinate: manifest.lemma.clone(),
        source: std::iter::repeat_n('0', 64).collect(),
        api: contract.api.clone(),
        abi: contract.abi.clone(),
        dependencies: Vec::new(),
    }
}

fn stage(
    files: &[(&str, &str)],
    version: &str,
    deps: &[&str],
) -> Result<(Manifest, Contract), Fault> {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), files);
    let man = manifest(version, deps);
    let contract = surface::extract(dir.path(), &man)?;
    Ok((man, contract))
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }
}

fn check(prev: (&Manifest, &Contract), next: (&Manifest, &Contract)) -> Result<Verdict, Fault> {
    let s1 = stamp(prev.0, prev.1);
    let s2 = stamp(next.0, next.1);
    audit::verify(
        Some(Release {
            seal: &s1,
            contract: prev.1,
        }),
        Release {
            seal: &s2,
            contract: next.1,
        },
        false,
    )
}

#[test]
fn implementation() {
    let v1 = "secret := 10\n\nhelper := (x: i64) -> i64 {\n    x + secret\n}\n\npub dylib compute := (a: i64, b: i64) -> i64 {\n    helper(a) + b\n}\n";
    let v2 = "hidden := 999\n\nPrivate := struct {\n    pad: i64\n}\n\nassist := (x: i64) -> i64 {\n    item := Private { pad: 4 }\n    ((x * hidden) - item.pad) + item.pad\n}\n\npub dylib compute := (a: i64, b: i64) -> i64 {\n    assist(a + b)\n}\n";
    let priv1 = "internal := 1\n";
    let priv2 = "internal := 2\nextra := (x: i64) -> i64 {\n    x + internal\n}\n";
    let (m1, c1) = stage(&[("parse.sx", v1), ("internal.sx", priv1)], "1.0.0", &[]).unwrap();
    let (m2, c2) = stage(
        &[
            ("parse.sx", v2),
            ("internal.sx", priv2),
            ("scratch.sx", "k := 7\n"),
        ],
        "1.0.1",
        &[],
    )
    .unwrap();
    assert_eq!(c1.exports, c2.exports);
    assert_eq!(c1.layouts, c2.layouts);
    assert_eq!(c1.api, c2.api);
    assert_eq!(c1.abi, c2.abi);
    assert_eq!(check((&m1, &c1), (&m2, &c2)), Ok(Verdict::Compatible));
}

#[test]
fn additive() {
    let v1 = "pub add := (a: i64, b: i64) -> i64 {\n    a + b\n}\n";
    let v2 = "pub Header := struct {\n    pub tag: u8\n    pub size: u32\n}\n\npub add := (a: i64, b: i64) -> i64 {\n    a + b\n}\n\npub mul := (a: i64, b: i64) -> i64 {\n    a * b\n}\n";
    let extra = "pub dylib encode := (x: i64) -> i64 {\n    x\n}\n";
    let (m1, c1) = stage(&[("parse.sx", v1)], "1.0.0", &[]).unwrap();
    let (m2, c2) = stage(&[("parse.sx", v2), ("format.sx", extra)], "1.0.1", &[]).unwrap();
    assert_ne!(c1.api, c2.api);
    assert_ne!(c1.abi, c2.abi);
    assert!(c2.exports.contains_key("format::bind:encode"));
    assert!(c2.layouts.contains_key("format::dylib:encode"));
    assert_eq!(check((&m1, &c1), (&m2, &c2)), Ok(Verdict::Compatible));
}

#[test]
fn signatures() {
    let base = "pub dylib encode := (x: i64) -> i64 {\n    x\n}\n";
    let altered = "pub dylib encode := (x: i32) -> i32 {\n    x\n}\n";
    let (m1, c1) = stage(&[("format.sx", base)], "1.0.0", &[]).unwrap();
    let (mz, cz) = stage(&[("format.sx", altered)], "1.0.1", &[]).unwrap();
    let (my, cy) = stage(&[("format.sx", altered)], "1.1.0", &[]).unwrap();
    assert!(c1.layouts.contains_key("format::dylib:encode"));
    match check((&m1, &c1), (&mz, &cz)) {
        Err(Fault::Breaking(msg)) => assert!(msg.contains("format::bind:encode")),
        other => panic!("expected breaking fault, got {other:?}"),
    }
    assert_eq!(check((&m1, &c1), (&my, &cy)), Ok(Verdict::Breaking));
}

#[test]
fn aggregates() {
    let base = concat!(
        "pub Packet := struct {\n    pub flag: u8\n    pub len: u32\n}\n\n",
        "pub State := enum {\n    Idle\n    Active(u64)\n}\n\n",
        "pub Stream := interface {\n    pull: (self: &var Self) -> i64\n    close: (self: &var Self)\n}\n"
    );
    let offset = concat!(
        "pub Packet := struct {\n    pub flag: u8\n    pad: u64\n    pub len: u32\n}\n\n",
        "pub State := enum {\n    Idle\n    Active(u64)\n}\n\n",
        "pub Stream := interface {\n    pull: (self: &var Self) -> i64\n    close: (self: &var Self)\n}\n"
    );
    let variant = concat!(
        "pub Packet := struct {\n    pub flag: u8\n    pub len: u32\n}\n\n",
        "pub State := enum {\n    Active(u64)\n    Idle\n}\n\n",
        "pub Stream := interface {\n    pull: (self: &var Self) -> i64\n    close: (self: &var Self)\n}\n"
    );
    let slot = concat!(
        "pub Packet := struct {\n    pub flag: u8\n    pub len: u32\n}\n\n",
        "pub State := enum {\n    Idle\n    Active(u64)\n}\n\n",
        "pub Stream := interface {\n    close: (self: &var Self)\n    pull: (self: &var Self) -> i64\n}\n"
    );

    let (m0, c0) = stage(&[("lexer/token.sx", base)], "1.0.0", &[]).unwrap();
    assert!(c0.layouts["lexer.token::struct:Packet"].contains("size=8;align=4;passing=register"));
    assert!(
        c0.layouts["lexer.token::enum:State"]
            .contains("discriminant=1;size=16;align=8;passing=pair")
    );
    assert!(c0.layouts["lexer.token::interface:Stream"].contains("0:pull"));

    for (source, key) in [
        (offset, "lexer.token::struct:Packet"),
        (variant, "lexer.token::enum:State"),
        (slot, "lexer.token::interface:Stream"),
    ] {
        let (mz, cz) = stage(&[("lexer/token.sx", source)], "1.0.1", &[]).unwrap();
        match check((&m0, &c0), (&mz, &cz)) {
            Err(Fault::Breaking(msg)) => assert!(msg.contains(key), "missing {key} in {msg}"),
            other => panic!("expected breaking on {key}, got {other:?}"),
        }
        let (my, cy) = stage(&[("lexer/token.sx", source)], "1.1.0", &[]).unwrap();
        assert_eq!(check((&m0, &c0), (&my, &cy)), Ok(Verdict::Breaking));
    }
}

#[test]
fn modules() {
    let parse = concat!(
        "wire := import wire\n",
        "text := import math.text\n",
        "format := dylib math.format\n\n",
        "pub Frame := struct {\n    pub head: wire.Header\n    pub text: text.Span\n}\n\n",
        "pub render := () -> format.Output {\n    format.make()\n}\n"
    );
    let wire = "pub Header := struct {\n    pub magic: u32\n    pub count: u32\n}\n";
    let (_, contract) = stage(
        &[
            ("parse.sx", parse),
            ("wire.sx", wire),
            ("nested/simplex.toml", "schema = \"simplex.lemma/v1\"\n"),
            ("nested/index.sx", "pub bad := 1\n"),
            (".hidden/ignored.sx", "INVALID\n"),
            ("target/ignored.sx", "INVALID\n"),
        ],
        "1.0.0",
        &["math"],
    )
    .unwrap();
    assert!(contract.exports.contains_key("parse::struct:Frame"));
    assert!(contract.exports.contains_key("wire::struct:Header"));
    assert!(contract.exports["parse::struct:Frame"].contains("ext:math.text.Span"));
    assert!(contract.exports["parse::bind:render"].contains("ext:math.format.Output"));
    assert!(contract.layouts["parse::struct:Frame"].contains("size=16;align=8;passing=pair"));
}

#[test]
fn guards() {
    let empty = tempfile::tempdir().unwrap();
    assert!(matches!(
        surface::extract(empty.path(), &manifest("1.0.0", &[])),
        Err(Fault::Surface(_))
    ));
    for bad in ["index.sx", "mod.sx", "lib.sx", "sub/mod.sx"] {
        let res = stage(&[(bad, "pub x := 1\n")], "1.0.0", &[]);
        assert!(matches!(res, Err(Fault::Surface(_))), "accepted {bad}");
    }
    let missing = stage(&[("parse.sx", "ghost := import ghost\n")], "1.0.0", &[]);
    assert!(matches!(missing, Err(Fault::Surface(_))));
    let singleseg = stage(&[("parse.sx", "math := import math\n")], "1.0.0", &["math"]);
    assert!(matches!(singleseg, Err(Fault::Surface(_))));
    let singledylib = stage(&[("parse.sx", "math := dylib math\n")], "1.0.0", &["math"]);
    assert!(matches!(singledylib, Err(Fault::Surface(_))));
    let shadowfile = stage(
        &[("parse.sx", "pub ok := 1\n"), ("math.sx", "pub bad := 2\n")],
        "1.0.0",
        &["math"],
    );
    assert!(matches!(shadowfile, Err(Fault::Surface(_))));
    let shadowdir = stage(
        &[
            ("parse.sx", "pub ok := 1\n"),
            ("math/sub.sx", "pub bad := 2\n"),
        ],
        "1.0.0",
        &["math"],
    );
    assert!(matches!(shadowdir, Err(Fault::Surface(_))));
}
