use simplex_package::{
    Contract, Coordinate, Fault, Locked, Manifest, Release, SEAL, Seal, Verdict, Version, archive,
    audit, surface,
};
use std::collections::BTreeMap;
use std::fs;

fn hex(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn coord(scope: &str, name: &str, ver: &str, channel: &str) -> Coordinate {
    Coordinate::new(scope, name, Version::parse(ver).unwrap(), channel).unwrap()
}

fn stamp(ver: &str) -> Seal {
    Seal {
        coordinate: coord("perish", "codec", ver, "stable"),
        source: hex('a'),
        api: hex('b'),
        abi: hex('c'),
        dependencies: Vec::new(),
    }
}

fn contract(exports: &[(&str, &str)], layouts: &[(&str, &str)]) -> Contract {
    let mut exp = BTreeMap::new();
    for (key, val) in exports {
        exp.insert((*key).to_owned(), (*key).to_owned() + "=" + val);
    }
    let mut lay = BTreeMap::new();
    for (key, val) in layouts {
        lay.insert((*key).to_owned(), (*key).to_owned() + "=" + val);
    }
    Contract {
        api: hex('d'),
        abi: hex('e'),
        exports: exp,
        layouts: lay,
    }
}

#[test]
fn round() {
    let seal = Seal {
        coordinate: coord("perish", "codec", "1.2.0", "stable"),
        source: hex('1'),
        api: hex('2'),
        abi: hex('3'),
        dependencies: vec![
            Locked {
                alias: String::from("wire"),
                coordinate: coord("perish", "wire", "0.9.3", "canary"),
                source: hex('4'),
                api: hex('5'),
                abi: hex('6'),
            },
            Locked {
                alias: String::from("math"),
                coordinate: coord("perish", "math", "0.4.1", "stable"),
                source: hex('7'),
                api: hex('8'),
                abi: hex('9'),
            },
        ],
    };
    let rendered = seal.render();
    assert!(rendered.starts_with(&format!("schema = \"{SEAL}\"\n")));
    let first = rendered.find("alias = \"math\"").unwrap();
    let second = rendered.find("alias = \"wire\"").unwrap();
    assert!(first < second);
    let parsed = Seal::parse(&rendered).unwrap();
    assert_eq!(parsed.coordinate, seal.coordinate);
    assert_eq!(parsed.source, seal.source);
    assert_eq!(parsed.api, seal.api);
    assert_eq!(parsed.abi, seal.abi);
    assert_eq!(parsed.dependencies.len(), 2);
    assert_eq!(parsed.dependencies[0].alias, "math");
    assert_eq!(parsed.dependencies[1].alias, "wire");
    assert_eq!(parsed.render(), rendered);
}

#[test]
fn validation() {
    let base = stamp("1.0.0").render();
    let wrong = base.replace("simplex.seal/v1", "simplex.seal/v2");
    assert!(matches!(Seal::parse(&wrong), Err(Fault::Seal(_))));

    let short = base.replace(&hex('a'), "abcd");
    assert!(matches!(Seal::parse(&short), Err(Fault::Seal(_))));

    let upper = base.replace(&hex('b'), &hex('B'));
    assert!(matches!(Seal::parse(&upper), Err(Fault::Seal(_))));

    let fuzzy = base.replace("1.0.0", "^1.0.0");
    assert!(Seal::parse(&fuzzy).is_err());

    let extra = format!("{base}extra = \"nope\"\n");
    assert!(matches!(Seal::parse(&extra), Err(Fault::Seal(_))));

    let dup = format!(
        "{base}\n[[dependencies]]\nalias = \"math\"\nscope = \"perish\"\nname = \"math\"\nversion = \"0.1.0\"\nchannel = \"stable\"\nsource = \"{}\"\napi = \"{}\"\nabi = \"{}\"\n\n[[dependencies]]\nalias = \"math\"\nscope = \"perish\"\nname = \"math\"\nversion = \"0.1.0\"\nchannel = \"stable\"\nsource = \"{}\"\napi = \"{}\"\nabi = \"{}\"\n",
        hex('1'),
        hex('2'),
        hex('3'),
        hex('1'),
        hex('2'),
        hex('3')
    );
    assert!(matches!(Seal::parse(&dup), Err(Fault::Seal(_))));
}

#[test]
fn governance() {
    let s100 = stamp("1.0.0");
    let c100 = contract(&[("bind:add", "fn(i64)->i64")], &[("struct:Pair", "16")]);
    let r100 = Release {
        seal: &s100,
        contract: &c100,
    };
    assert_eq!(audit::verify(None, r100, false), Ok(Verdict::Initial));

    let s101 = stamp("1.0.1");
    let c101 = contract(
        &[("bind:add", "fn(i64)->i64"), ("bind:sub", "fn(i64)->i64")],
        &[("struct:Pair", "16")],
    );
    let r101 = Release {
        seal: &s101,
        contract: &c101,
    };
    assert_eq!(
        audit::verify(Some(r100), r101, false),
        Ok(Verdict::Compatible)
    );

    let s102 = stamp("1.0.2");
    let broken = contract(&[("bind:add", "fn(i32)->i32")], &[("struct:Pair", "16")]);
    let bad = audit::verify(
        Some(r101),
        Release {
            seal: &s102,
            contract: &broken,
        },
        false,
    );
    match bad {
        Err(Fault::Breaking(msg)) => assert!(msg.contains("bind:add")),
        other => panic!("expected breaking fault, got {other:?}"),
    }

    let s110 = stamp("1.1.0");
    let r110 = Release {
        seal: &s110,
        contract: &broken,
    };
    assert_eq!(
        audit::verify(Some(r101), r110, false),
        Ok(Verdict::Breaking)
    );

    let s200 = stamp("2.0.0");
    let r200 = Release {
        seal: &s200,
        contract: &broken,
    };
    assert!(matches!(
        audit::verify(Some(r110), r200, false),
        Err(Fault::Epoch(_))
    ));
    assert_eq!(audit::verify(Some(r110), r200, true), Ok(Verdict::Epoch));
    assert!(matches!(
        audit::verify(Some(r200), r110, true),
        Err(Fault::Version(_))
    ));
    assert!(matches!(
        audit::verify(Some(r200), r200, true),
        Err(Fault::Version(_))
    ));

    let foreign = Seal {
        coordinate: coord("perish", "other", "2.1.0", "stable"),
        source: hex('a'),
        api: hex('b'),
        abi: hex('c'),
        dependencies: Vec::new(),
    };
    assert!(matches!(
        audit::verify(
            Some(r200),
            Release {
                seal: &foreign,
                contract: &broken,
            },
            false
        ),
        Err(Fault::Version(_))
    ));
}

#[test]
fn pipeline() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("simplex.toml"),
        "schema = \"simplex.package/v1\"\n\n[library]\nscope = \"perish\"\nname = \"codec\"\nversion = \"0.1.0\"\nchannel = \"stable\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("codec.sx"),
        concat!(
            "pub limit := 64\n\n",
            "pub Word := u64\n\n",
            "pub dylib puts := extern \"c\" \"puts\" fn(ptr: &u8) -> i32\n"
        ),
    )
    .unwrap();
    let manifest = Manifest::load(dir.path()).unwrap();
    let (_, source) = archive::pack(dir.path(), &manifest).unwrap();
    let surface = surface::extract(dir.path(), &manifest).unwrap();
    assert!(surface.exports.contains_key("bind:limit"));
    assert!(surface.exports.contains_key("alias:Word"));
    assert!(surface.exports.contains_key("extern:puts"));
    assert!(surface.layouts.contains_key("dylib:puts"));
    let seal = Seal {
        coordinate: manifest.library,
        source,
        api: surface.api.clone(),
        abi: surface.abi.clone(),
        dependencies: Vec::new(),
    };
    let parsed = Seal::parse(&seal.render()).unwrap();
    assert_eq!(parsed, seal);
}
