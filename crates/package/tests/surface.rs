use simplex_package::{
    Contract, Coordinate, Dependency, Fault, Manifest, Release, Seal, Verdict, Version, audit,
    surface,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn manifest(version: &str, deps: &[&str]) -> Manifest {
    let parsed = Version::parse(version).unwrap();
    let library = Coordinate::new("perish", "codec", parsed, "stable").unwrap();
    let mut dependencies = BTreeMap::new();
    for name in deps {
        let dep =
            Coordinate::new("perish", name, Version::parse("1.0.0").unwrap(), "stable").unwrap();
        dependencies.insert((*name).to_owned(), Dependency::Exact(dep));
    }
    Manifest {
        library,
        entry: String::from("codec.sx"),
        dependencies,
    }
}

fn stamp(manifest: &Manifest, contract: &Contract) -> Seal {
    Seal {
        coordinate: manifest.library.clone(),
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

#[test]
fn implementation() {
    let v1 = concat!(
        "secret := 10\n\n",
        "helper := (x: i64) -> i64 {\n    x + secret\n}\n\n",
        "pub dylib compute := (a: i64, b: i64) -> i64 {\n    helper(a) + b\n}\n"
    );
    let v2 = concat!(
        "hidden := 999\n\n",
        "Private := struct {\n    pad: i64\n}\n\n",
        "assist := (x: i64) -> i64 {\n    item := Private { pad: 4 }\n    ((x * hidden) - item.pad) + item.pad\n}\n\n",
        "pub dylib compute := (a: i64, b: i64) -> i64 {\n    assist(a + b)\n}\n"
    );
    let (m1, c1) = stage(&[("codec.sx", v1)], "1.0.0", &[]).unwrap();
    let (m2, c2) = stage(&[("codec.sx", v2)], "1.0.1", &[]).unwrap();
    assert_eq!(c1.exports, c2.exports);
    assert_eq!(c1.layouts, c2.layouts);
    assert_eq!(c1.api, c2.api);
    assert_eq!(c1.abi, c2.abi);

    let s1 = stamp(&m1, &c1);
    let s2 = stamp(&m2, &c2);
    let verdict = audit::verify(
        Some(Release {
            seal: &s1,
            contract: &c1,
        }),
        Release {
            seal: &s2,
            contract: &c2,
        },
        false,
    );
    assert_eq!(verdict, Ok(Verdict::Compatible));
}

#[test]
fn additive() {
    let v1 = "pub add := (a: i64, b: i64) -> i64 {\n    a + b\n}\n";
    let v2 = concat!(
        "pub Header := struct {\n    pub tag: u8\n    pub size: u32\n}\n\n",
        "pub add := (a: i64, b: i64) -> i64 {\n    a + b\n}\n\n",
        "pub mul := (a: i64, b: i64) -> i64 {\n    a * b\n}\n"
    );
    let (m1, c1) = stage(&[("codec.sx", v1)], "1.0.0", &[]).unwrap();
    let (m2, c2) = stage(&[("codec.sx", v2)], "1.0.1", &[]).unwrap();
    assert_ne!(c1.api, c2.api);
    assert_ne!(c1.abi, c2.abi);
    let s1 = stamp(&m1, &c1);
    let s2 = stamp(&m2, &c2);
    assert_eq!(
        audit::verify(
            Some(Release {
                seal: &s1,
                contract: &c1
            }),
            Release {
                seal: &s2,
                contract: &c2
            },
            false
        ),
        Ok(Verdict::Compatible)
    );
}

#[test]
fn signatures() {
    let base = "pub dylib encode := (x: i64) -> i64 {\n    x\n}\n";
    let altered = "pub dylib encode := (x: i32) -> i32 {\n    x\n}\n";
    let (m1, c1) = stage(&[("codec.sx", base)], "1.0.0", &[]).unwrap();
    let (mz, cz) = stage(&[("codec.sx", altered)], "1.0.1", &[]).unwrap();
    let (my, cy) = stage(&[("codec.sx", altered)], "1.1.0", &[]).unwrap();
    let s1 = stamp(&m1, &c1);
    let sz = stamp(&mz, &cz);
    let sy = stamp(&my, &cy);

    match audit::verify(
        Some(Release {
            seal: &s1,
            contract: &c1,
        }),
        Release {
            seal: &sz,
            contract: &cz,
        },
        false,
    ) {
        Err(Fault::Breaking(msg)) => assert!(msg.contains("encode")),
        other => panic!("expected breaking fault, got {other:?}"),
    }
    assert_eq!(
        audit::verify(
            Some(Release {
                seal: &s1,
                contract: &c1
            }),
            Release {
                seal: &sy,
                contract: &cy
            },
            false
        ),
        Ok(Verdict::Breaking)
    );
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

    let (m0, c0) = stage(&[("codec.sx", base)], "1.0.0", &[]).unwrap();
    let s0 = stamp(&m0, &c0);
    assert!(c0.layouts["struct:Packet"].contains("size=8;align=4;passing=register"));
    assert!(c0.layouts["enum:State"].contains("discriminant=1;size=16;align=8;passing=pair"));
    assert!(c0.layouts["interface:Stream"].contains("0:pull"));

    for (source, key) in [
        (offset, "struct:Packet"),
        (variant, "enum:State"),
        (slot, "interface:Stream"),
    ] {
        let (mz, cz) = stage(&[("codec.sx", source)], "1.0.1", &[]).unwrap();
        let sz = stamp(&mz, &cz);
        match audit::verify(
            Some(Release {
                seal: &s0,
                contract: &c0,
            }),
            Release {
                seal: &sz,
                contract: &cz,
            },
            false,
        ) {
            Err(Fault::Breaking(msg)) => assert!(msg.contains(key), "missing {key} in {msg}"),
            other => panic!("expected breaking on {key}, got {other:?}"),
        }

        let (my, cy) = stage(&[("codec.sx", source)], "1.1.0", &[]).unwrap();
        let sy = stamp(&my, &cy);
        assert_eq!(
            audit::verify(
                Some(Release {
                    seal: &s0,
                    contract: &c0
                }),
                Release {
                    seal: &sy,
                    contract: &cy
                },
                false
            ),
            Ok(Verdict::Breaking)
        );
    }
}

#[test]
fn modules() {
    let entry = concat!(
        "wire := import wire\n",
        "math := import math\n\n",
        "pub Frame := struct {\n    pub head: wire.Header\n}\n"
    );
    let wire = "pub Header := struct {\n    pub magic: u32\n    pub count: u32\n}\n";
    let stray = "THIS IS UNREACHABLE AND NOT PARSED\n";
    let (_, contract) = stage(
        &[("codec.sx", entry), ("wire.sx", wire), ("stray.sx", stray)],
        "1.0.0",
        &["math"],
    )
    .unwrap();
    assert!(contract.exports.contains_key("struct:Frame"));
    assert!(contract.exports.contains_key("wire::struct:Header"));
    assert!(contract.layouts["struct:Frame"].contains("size=8;align=4;passing=register"));

    let missing = stage(&[("codec.sx", "ghost := import ghost\n")], "1.0.0", &[]);
    assert!(matches!(missing, Err(Fault::Surface(_))));
}
