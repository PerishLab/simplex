use super::support::{expectcode, node, write};
use simplex::Code;
use simplex_hir::{Fault, Graph, build, register};
use simplex_package::Coordinate;
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn visibility() {
    let scratch = tempdir().expect("tempdir");
    let depdir = scratch.path().join("dep");
    write(
        &depdir,
        "parse.sx",
        concat!(
            "Hidden := struct {\n    code: i64\n}\n\n",
            "pub Partial := struct {\n    pub open: i64\n    secret: i64\n}\n\n",
            "internal := () -> i64 {\n    9\n}\n\n",
            "pub make := (open: i64) -> Partial {\n",
            "    Partial { open: open, secret: internal() }\n",
            "}\n",
        ),
    );
    let cases = [
        "parse := import dep.parse\n\npub main := () -> i64 {\n    parse.internal()\n}\n",
        "parse := import dep.parse\n\nHolder := struct {\n    item: parse.Hidden\n}\n\npub main := () -> i64 {\n    0\n}\n",
        "parse := import dep.parse\n\npub main := () -> i64 {\n    var p := parse.Partial { open: 1, secret: 2 }\n    p.open\n}\n",
        "parse := import dep.parse\n\npub main := () -> i64 {\n    var p := parse.make(4)\n    p.secret\n}\n",
    ];
    for (index, source) in cases.iter().enumerate() {
        let appdir = scratch.path().join(format!("vis{index}"));
        write(&appdir, "main.sx", source);
        let dep = node("perish/dep/1.0.0/stable", &depdir, &[]);
        let root = node(
            "perish/app/1.0.0/stable",
            &appdir,
            &[("dep", "perish/dep/1.0.0/stable")],
        );
        let mut packages = BTreeMap::new();
        packages.insert(dep.coordinate.clone(), dep);
        expectcode(&Graph { root, packages }, Code::Visibility);
    }
}

#[test]
fn boundaries() {
    let scratch = tempdir().expect("tempdir");
    let depdir = scratch.path().join("dep");
    write(
        &depdir,
        "format.sx",
        concat!(
            "pub Exposed := struct {\n    pub tag: i64\n}\n\n",
            "pub helper := (x: i64) -> i64 {\n    x + 1\n}\n\n",
            "pub dylib open := (x: i64) -> i64 {\n    helper(x)\n}\n",
        ),
    );
    let cases = [
        "format := dylib dep.format\n\npub main := () -> i64 {\n    format.helper(1)\n}\n",
        "format := dylib dep.format\n\nWrap := struct {\n    inner: format.Exposed\n}\n\npub main := () -> i64 {\n    0\n}\n",
        "format := dylib dep.format\n\npub main := () -> i64 {\n    var s := format.Exposed { tag: 3 }\n    s.tag\n}\n",
    ];
    for (index, source) in cases.iter().enumerate() {
        let appdir = scratch.path().join(format!("bnd{index}"));
        write(&appdir, "main.sx", source);
        let dep = node("perish/dep/1.0.0/stable", &depdir, &[]);
        let root = node(
            "perish/app/1.0.0/stable",
            &appdir,
            &[("dep", "perish/dep/1.0.0/stable")],
        );
        let mut packages = BTreeMap::new();
        packages.insert(dep.coordinate.clone(), dep);
        expectcode(&Graph { root, packages }, Code::Boundary);
    }
}

#[test]
fn topology() {
    let scratch = tempdir().expect("tempdir");
    let adir = scratch.path().join("a");
    let bdir = scratch.path().join("b");
    let cdir = scratch.path().join("c");
    let ddir = scratch.path().join("d");
    let missdir = scratch.path().join("miss");
    write(
        &adir,
        "main.sx",
        "node := import b.node\n\npub main := () -> i64 {\n    0\n}\n",
    );
    write(
        &bdir,
        "node.sx",
        "main := import a.main\n\npub ping := () -> i64 {\n    1\n}\n",
    );
    let apkg = node(
        "perish/a/1.0.0/stable",
        &adir,
        &[("b", "perish/b/1.0.0/stable")],
    );
    let bpkg = node(
        "perish/b/1.0.0/stable",
        &bdir,
        &[("a", "perish/a/1.0.0/stable")],
    );
    let mut packages = BTreeMap::new();
    packages.insert(bpkg.coordinate.clone(), bpkg);
    expectcode(
        &Graph {
            root: apkg,
            packages,
        },
        Code::Cycle,
    );

    write(
        &cdir,
        "main.sx",
        "node := dylib d.node\n\npub main := () -> i64 {\n    0\n}\n",
    );
    write(
        &ddir,
        "node.sx",
        "main := dylib c.main\n\npub dylib ping := () -> u64 {\n    1\n}\n",
    );
    let cpkg = node(
        "perish/c/1.0.0/stable",
        &cdir,
        &[("d", "perish/d/1.0.0/stable")],
    );
    let dpkg = node(
        "perish/d/1.0.0/stable",
        &ddir,
        &[("c", "perish/c/1.0.0/stable")],
    );
    let mut dynpkgs = BTreeMap::new();
    dynpkgs.insert(dpkg.coordinate.clone(), dpkg);
    expectcode(
        &Graph {
            root: cpkg,
            packages: dynpkgs,
        },
        Code::Cycle,
    );

    write(
        &missdir,
        "main.sx",
        "node := import ghost.node\n\npub main := () -> i64 {\n    0\n}\n",
    );
    let orphan = node(
        "perish/app/1.0.0/stable",
        &missdir,
        &[("ghost", "perish/ghost/1.0.0/stable")],
    );
    expectcode(
        &Graph {
            root: orphan,
            packages: BTreeMap::new(),
        },
        Code::Import,
    );
}

#[test]
fn morphology() {
    let scratch = tempdir().expect("tempdir");
    let jsondir = scratch.path().join("json");
    write(&jsondir, "parse.sx", "pub parse := () -> i64 {\n    1\n}\n");
    write(
        &jsondir,
        "format.sx",
        "pub dylib fmt := () -> i64 {\n    2\n}\n",
    );
    let json = node("perish/codec.json/1.0.0/stable", &jsondir, &[]);

    let single = scratch.path().join("single");
    write(
        &single,
        "main.sx",
        "json := import json\n\npub main := () -> i64 {\n    0\n}\n",
    );
    let mut packages = BTreeMap::new();
    packages.insert(json.coordinate.clone(), json.clone());
    expectcode(
        &Graph {
            root: node(
                "perish/app/1.0.0/stable",
                &single,
                &[("json", "perish/codec.json/1.0.0/stable")],
            ),
            packages: packages.clone(),
        },
        Code::Import,
    );

    let singledyn = scratch.path().join("singledyn");
    write(
        &singledyn,
        "main.sx",
        "json := dylib json\n\npub main := () -> i64 {\n    0\n}\n",
    );
    expectcode(
        &Graph {
            root: node(
                "perish/app/1.0.0/stable",
                &singledyn,
                &[("json", "perish/codec.json/1.0.0/stable")],
            ),
            packages: packages.clone(),
        },
        Code::Boundary,
    );

    let shadowfile = scratch.path().join("shadowfile");
    write(&shadowfile, "json.sx", "pub x := () -> i64 {\n    1\n}\n");
    write(
        &shadowfile,
        "main.sx",
        "parse := import json.parse\n\npub main := () -> i64 {\n    parse.parse()\n}\n",
    );
    expectcode(
        &Graph {
            root: node(
                "perish/app/1.0.0/stable",
                &shadowfile,
                &[("json", "perish/codec.json/1.0.0/stable")],
            ),
            packages: packages.clone(),
        },
        Code::Import,
    );

    let shadowdir = scratch.path().join("shadowdir");
    write(
        &shadowdir,
        "json/parse.sx",
        "pub parse := () -> i64 {\n    1\n}\n",
    );
    write(
        &shadowdir,
        "main.sx",
        "parse := import json.parse\n\npub main := () -> i64 {\n    parse.parse()\n}\n",
    );
    expectcode(
        &Graph {
            root: node(
                "perish/app/1.0.0/stable",
                &shadowdir,
                &[("json", "perish/codec.json/1.0.0/stable")],
            ),
            packages,
        },
        Code::Import,
    );

    let nomain = scratch.path().join("nomain");
    write(&nomain, "util.sx", "pub x := () -> i64 {\n    1\n}\n");
    let missing = build(
        &Graph {
            root: node("perish/app/1.0.0/stable", &nomain, &[]),
            packages: BTreeMap::new(),
        },
        false,
    );
    assert!(matches!(missing, Err(Fault::Io(_))));

    let emptydir = scratch.path().join("empty");
    std::fs::create_dir_all(&emptydir).expect("mkdir");
    let empty = build(
        &Graph {
            root: node("perish/app/1.0.0/stable", &emptydir, &[]),
            packages: BTreeMap::new(),
        },
        true,
    );
    assert!(matches!(empty, Err(Fault::Io(_))));
}

#[test]
fn collision() {
    let first = Coordinate::parse("perish/codec.json/1.0.0/stable").unwrap();
    let second = Coordinate::parse("perish/codec.text/1.0.0/stable").unwrap();
    let mut seen = BTreeMap::new();
    register(&mut seen, &first.stem(), &first).expect("first register");
    register(&mut seen, &first.stem(), &first).expect("idempotent register");
    let err = register(&mut seen, &first.stem(), &second).expect_err("must reject collision");
    assert!(matches!(err, Fault::Collision(_)));
    assert!(err.to_string().contains(&first.stem()));
}
