mod mock;

use mock::fixture::{bundle, sample};
use mock::server::Server;
use simplex_package::{Coordinate, Endpoint, Fault, Store, Version};
use std::fs;
use tempfile::tempdir;

#[test]
fn lifecycle() {
    let mut server = Server::start("registry", "auto", Some(sample()), None);
    let endpoint = Endpoint {
        url: server.url().into(),
        bucket: "registry".into(),
        region: "auto".into(),
        credentials: Some(sample()),
    };
    let coord = Coordinate::parse("perish/codec/1.0.0/stable").expect("coord");
    let (archive, seal) = bundle(&coord, "pub answer :: () -> i64 { 42 }\n");

    let producer = tempdir().expect("producer cache");
    let publisher = Store::new(producer.path().to_path_buf(), Some(endpoint.clone()));
    publisher.publish(&archive, &seal).expect("publish");

    assert_eq!(
        server.keys(),
        vec![
            "perish/codec/1.0.0/stable/seal.toml".to_owned(),
            "perish/codec/1.0.0/stable/source.tar".to_owned(),
        ]
    );
    let records = server.records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].method, "PUT");
    assert_eq!(records[1].method, "PUT");
    assert!(
        records[0]
            .target
            .ends_with("/perish/codec/1.0.0/stable/source.tar")
    );
    assert!(
        records[1]
            .target
            .ends_with("/perish/codec/1.0.0/stable/seal.toml")
    );
    assert_eq!(
        records[0].headers.get("if-none-match").map(String::as_str),
        Some("*")
    );
    assert_eq!(
        records[1].headers.get("if-none-match").map(String::as_str),
        Some("*")
    );
    assert!(matches!(
        publisher.publish(&archive, &seal),
        Err(Fault::Immutable(_))
    ));

    let consumer = tempdir().expect("consumer cache");
    let fetcher = Store::new(consumer.path().to_path_buf(), Some(endpoint));
    let fetched = fetcher.fetch(&coord).expect("remote fetch");
    assert_eq!(fetched.coordinate, coord);
    assert_eq!(fetched.archive, archive);
    assert_eq!(fetched.seal, seal);
    assert!(fetched.root.join("codec.sx").exists());
    assert_eq!(
        fs::read(fetched.root.join("asset.bin")).expect("read asset"),
        b"payload-bytes"
    );

    server.shutdown();
    let offline = Store::new(consumer.path().to_path_buf(), None);
    assert_eq!(offline.fetch(&coord).expect("offline cache hit").seal, seal);

    let uncached = Coordinate::parse("perish/codec/2.0.0/stable").expect("uncached");
    assert!(matches!(offline.fetch(&uncached), Err(Fault::Offline(_))));
}

#[test]
fn integrity() {
    let server = Server::start("registry", "auto", Some(sample()), None);
    let endpoint = Endpoint {
        url: server.url().into(),
        bucket: "registry".into(),
        region: "auto".into(),
        credentials: Some(sample()),
    };
    let coord = Coordinate::parse("perish/wire/0.2.0/stable").expect("coord");
    let other = Coordinate::parse("perish/wire/0.2.0/canary").expect("other");
    let (archive, seal) = bundle(&coord, "pub magic :: () -> i64 { 7 }\n");
    let (_, mismatched) = bundle(&other, "pub magic :: () -> i64 { 7 }\n");

    let home = tempdir().expect("cache");
    let store = Store::new(home.path().to_path_buf(), Some(endpoint));

    let mut wrong = archive.clone();
    wrong.push(0xff);
    assert!(matches!(
        store.publish(&wrong, &seal),
        Err(Fault::Integrity(_))
    ));
    assert!(server.keys().is_empty());

    server.insert(
        "perish/wire/0.2.0/stable/seal.toml",
        mismatched.render().into_bytes(),
    );
    assert!(matches!(store.fetch(&coord), Err(Fault::Integrity(_))));

    server.insert(
        "perish/wire/0.2.0/stable/seal.toml",
        seal.render().into_bytes(),
    );
    server.insert("perish/wire/0.2.0/stable/source.tar", wrong);
    assert!(matches!(store.fetch(&coord), Err(Fault::Integrity(_))));
    assert!(
        !home
            .path()
            .join("perish/wire/0.2.0/stable/seal.toml")
            .exists()
    );

    server.insert("perish/wire/0.2.0/stable/source.tar", archive);
    assert_eq!(store.fetch(&coord).expect("clean fetch").coordinate, coord);

    let cached = home.path().join("perish/wire/0.2.0/stable/source.tar");
    fs::write(&cached, b"corrupted-tar-bytes").expect("corrupt local tar");
    assert!(matches!(store.fetch(&coord), Err(Fault::Integrity(_))));

    fs::remove_file(&cached).expect("remove tar");
    assert!(matches!(store.fetch(&coord), Err(Fault::Integrity(_))));
}

#[test]
fn isolation() {
    let server = Server::start("public", "auto", None, Some(2));
    let endpoint = Endpoint {
        url: server.url().into(),
        bucket: "public".into(),
        region: "auto".into(),
        credentials: None,
    };
    for spec in [
        "perish/math/0.9.0/stable",
        "perish/math/1.2.0/stable",
        "perish/math/1.10.0/stable",
        "perish/math/9.0.0/canary",
    ] {
        let coord = Coordinate::parse(spec).expect("coord");
        let (archive, seal) = bundle(&coord, "pub unit :: () -> i64 { 1 }\n");
        let prefix = coord.prefix();
        server.insert(&format!("{prefix}/source.tar"), archive);
        server.insert(&format!("{prefix}/seal.toml"), seal.render().into_bytes());
    }
    server.insert("perish/math/8.8.8/stable/source.tar", b"orphan".to_vec());

    let home = tempdir().expect("cache");
    let store = Store::new(home.path().to_path_buf(), Some(endpoint));

    let canary = Coordinate::parse("perish/math/9.0.0/canary").expect("canary");
    let (raw, signed) = bundle(&canary, "pub unit :: () -> i64 { 1 }\n");
    assert!(matches!(
        store.publish(&raw, &signed),
        Err(Fault::Remote(_))
    ));

    let missing = Coordinate::parse("perish/math/9.0.0/stable").expect("missing stable");
    assert!(matches!(store.fetch(&missing), Err(Fault::Missing(_))));

    let patch = Coordinate::parse("perish/math/1.2.1/stable").expect("missing patch");
    assert!(matches!(store.fetch(&patch), Err(Fault::Missing(_))));
    assert_eq!(
        store.fetch(&canary).expect("public fetch").coordinate,
        canary
    );

    let best = store.latest("perish", "math", "stable").expect("latest");
    assert_eq!(
        best,
        Some(Version {
            major: 1,
            minor: 10,
            patch: 0
        })
    );
    assert_eq!(store.latest("perish", "math", "rc").expect("rc"), None);
}
