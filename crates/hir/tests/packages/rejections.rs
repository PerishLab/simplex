use super::support::{expectcode, node, write};
use simplex::Code;
use simplex_hir::Graph;
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn visibility() {
    let scratch = tempdir().expect("tempdir");
    let depdir = scratch.path().join("dep");
    write(
        &depdir,
        "dep.sx",
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
        "dep := import dep\n\npub main := () -> i64 {\n    dep.internal()\n}\n",
        "dep := import dep\n\nHolder := struct {\n    item: dep.Hidden\n}\n\npub main := () -> i64 {\n    0\n}\n",
        "dep := import dep\n\npub main := () -> i64 {\n    var p := dep.Partial { open: 1, secret: 2 }\n    p.open\n}\n",
        "dep := import dep\n\npub main := () -> i64 {\n    var p := dep.make(4)\n    p.secret\n}\n",
    ];
    for (index, source) in cases.iter().enumerate() {
        let appdir = scratch.path().join(format!("vis{index}"));
        write(&appdir, "app.sx", source);
        let dep = node("perish/dep/1.0.0/stable", &depdir, "dep.sx", &[]);
        let root = node(
            "perish/app/1.0.0/stable",
            &appdir,
            "app.sx",
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
        "dep.sx",
        concat!(
            "pub Exposed := struct {\n    pub tag: i64\n}\n\n",
            "pub helper := (x: i64) -> i64 {\n    x + 1\n}\n\n",
            "pub dylib open := (x: i64) -> i64 {\n    helper(x)\n}\n",
        ),
    );
    let cases = [
        "dep := dylib dep\n\npub main := () -> i64 {\n    dep.helper(1)\n}\n",
        "dep := dylib dep\n\nWrap := struct {\n    inner: dep.Exposed\n}\n\npub main := () -> i64 {\n    0\n}\n",
        "dep := dylib dep\n\npub main := () -> i64 {\n    var s := dep.Exposed { tag: 3 }\n    s.tag\n}\n",
    ];
    for (index, source) in cases.iter().enumerate() {
        let appdir = scratch.path().join(format!("bnd{index}"));
        write(&appdir, "app.sx", source);
        let dep = node("perish/dep/1.0.0/stable", &depdir, "dep.sx", &[]);
        let root = node(
            "perish/app/1.0.0/stable",
            &appdir,
            "app.sx",
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
        "a.sx",
        "b := import b\n\npub main := () -> i64 {\n    0\n}\n",
    );
    write(
        &bdir,
        "b.sx",
        "a := import a\n\npub ping := () -> i64 {\n    1\n}\n",
    );
    let apkg = node(
        "perish/a/1.0.0/stable",
        &adir,
        "a.sx",
        &[("b", "perish/b/1.0.0/stable")],
    );
    let bpkg = node(
        "perish/b/1.0.0/stable",
        &bdir,
        "b.sx",
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
        "c.sx",
        "d := dylib d\n\npub main := () -> i64 {\n    0\n}\n",
    );
    write(
        &ddir,
        "d.sx",
        "c := dylib c\n\npub dylib ping := () -> u64 {\n    1\n}\n",
    );
    let cpkg = node(
        "perish/c/1.0.0/stable",
        &cdir,
        "c.sx",
        &[("d", "perish/d/1.0.0/stable")],
    );
    let dpkg = node(
        "perish/d/1.0.0/stable",
        &ddir,
        "d.sx",
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
        "app.sx",
        "ghost := import ghost\n\npub main := () -> i64 {\n    0\n}\n",
    );
    let orphan = node(
        "perish/app/1.0.0/stable",
        &missdir,
        "app.sx",
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
