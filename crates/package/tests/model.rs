use sha2::{Digest, Sha256};
use simplex_package::{Coordinate, Fault, Version, model};
use std::fmt::Write;

fn expected(prefix: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"simplex.dylib/v1\0");
    hasher.update(prefix.as_bytes());
    let sum = hasher.finalize();
    let mut out = String::with_capacity(16);
    for byte in &sum[..8] {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[test]
fn versions() {
    let zero = Version::parse("0.0.0").expect("zero");
    assert_eq!(zero.to_string(), "0.0.0");
    let full = Version::parse("12.34.56").expect("full");
    assert_eq!(
        full,
        Version {
            major: 12,
            minor: 34,
            patch: 56
        }
    );
    assert_eq!(full.to_string(), "12.34.56");
    let huge = Version::parse("18446744073709551615.0.1").expect("u64 max");
    assert_eq!(huge.major, u64::MAX);
    assert!(zero < full && full < huge);
}

#[test]
fn malformed() {
    for bad in [
        "",
        "1",
        "1.2",
        "1.2.3.4",
        "1.2.",
        ".1.2",
        "01.0.0",
        "0.01.0",
        "0.0.01",
        "00.0.0",
        "1.2.3-rc.1",
        "1.2.3-alpha",
        "1.2.3+meta",
        "v1.2.3",
        "+1.2.3",
        "-1.2.3",
        " 1.2.3",
        "1.2.3 ",
        "1.2.x",
        "99999999999999999999999999.0.0",
    ] {
        assert!(
            matches!(Version::parse(bad), Err(Fault::Version(_))),
            "accepted {bad}"
        );
    }
}

#[test]
fn coordinates() {
    let coord = Coordinate::parse("perish/codec/1.2.0/stable").expect("coord");
    assert_eq!(coord.scope, "perish");
    assert_eq!(coord.name, "codec");
    assert_eq!(coord.leaf(), "codec");
    assert_eq!(coord.channel, "stable");
    assert_eq!(coord.prefix(), "perish/codec/1.2.0/stable");
    assert_eq!(coord.to_string(), "perish/codec/1.2.0/stable");
    let digest = expected("perish/codec/1.2.0/stable");
    assert_eq!(digest.len(), 16);
    assert!(
        digest
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_eq!(coord.hash(), digest);
    assert_eq!(coord.namespace(), digest);
    assert_eq!(coord.stem(), digest);

    let sub = Coordinate::parse("perish/codec.json/1.0.0/stable").expect("two level");
    assert_eq!(sub.name, "codec.json");
    assert_eq!(sub.leaf(), "json");
    assert_eq!(sub.prefix(), "perish/codec.json/1.0.0/stable");
    let subhash = expected("perish/codec.json/1.0.0/stable");
    assert_eq!(sub.hash(), subhash);
    assert_eq!(sub.namespace(), subhash);
    assert_eq!(sub.stem(), subhash);
    assert_ne!(coord.hash(), sub.hash());

    let digits = Coordinate::new(
        "org2",
        "codec2.lib9",
        Version {
            major: 0,
            minor: 4,
            patch: 1,
        },
        "rc1",
    )
    .expect("digits tail");
    assert_eq!(digits.prefix(), "org2/codec2.lib9/0.4.1/rc1");
    assert_eq!(digits.leaf(), "lib9");
}

#[test]
fn words() {
    let ver = Version::parse("1.0.0").expect("ver");
    for bad in [
        "", "Perish", "my_word", "my-word", "my.word", "my/word", "1word", " word", "word ",
    ] {
        assert!(matches!(
            Coordinate::new(bad, "codec", ver, "stable"),
            Err(Fault::Coordinate(_))
        ));
        assert!(matches!(
            Coordinate::new("perish", "codec", ver, bad),
            Err(Fault::Coordinate(_))
        ));
    }
    for bad in [
        "",
        "a.b.c",
        ".json",
        "codec.",
        "..",
        "codec..json",
        "my_word",
        "my-word",
        "my/word",
        "Codec",
        "codec.Json",
        "1word",
        "codec.1json",
        " word",
        "word ",
    ] {
        assert!(!model::name(bad), "accepted name {bad}");
        assert!(matches!(
            Coordinate::new("perish", bad, ver, "stable"),
            Err(Fault::Coordinate(_))
        ));
    }
    assert!(model::name("codec"));
    assert!(model::name("codec.json"));
    for text in [
        "",
        "perish/codec/1.0.0",
        "perish/codec/1.0.0/stable/extra",
        "/perish/codec/1.0.0/stable",
        "perish/codec/1.0.0/stable/",
        "Perish/codec/1.0.0/stable",
        "perish/codec/01.0.0/stable",
        "perish/a.b.c/1.0.0/stable",
        "perish/.json/1.0.0/stable",
        "perish/codec./1.0.0/stable",
    ] {
        assert!(Coordinate::parse(text).is_err(), "accepted {text}");
    }
}
