use super::support::{check, node, write};
use simplex_hir::{Graph, Mangle, build};
use simplex_package::Coordinate;
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn multichannel() {
    let scratch = tempdir().expect("tempdir");
    let sharedir = scratch.path().join("span");
    let stabledir = scratch.path().join("stable");
    let canarydir = scratch.path().join("canary");
    let vtwodir = scratch.path().join("vtwo");
    let leftdir = scratch.path().join("left");
    let rightdir = scratch.path().join("right");
    let appdir = scratch.path().join("app");
    write(
        &sharedir,
        "span.sx",
        "pub Span := struct {\n    pub start: i64\n    pub length: i64\n}\n",
    );
    write(
        &stabledir,
        "util.sx",
        "pub factor := () -> i64 {\n    2\n}\n",
    );
    write(
        &stabledir,
        "stable.sx",
        concat!(
            "util := import util\n\n",
            "pub Vector := struct {\n    pub x: i64\n    pub y: i64\n}\n\n",
            "pub sum := (v: Vector) -> i64 {\n    (v.x + v.y) * util.factor()\n}\n",
        ),
    );
    write(
        &canarydir,
        "util.sx",
        "pub factor := () -> i64 {\n    3\n}\n",
    );
    write(
        &canarydir,
        "canary.sx",
        concat!(
            "util := import util\n\n",
            "pub Vector := struct {\n    pub x: i64\n    pub y: i64\n    pub z: i64\n}\n\n",
            "pub sum := (v: Vector) -> i64 {\n    (v.x + v.y + v.z) * util.factor()\n}\n",
        ),
    );
    write(
        &vtwodir,
        "modern.sx",
        "pub sum := (seed: i64) -> i64 {\n    seed * 10\n}\n",
    );
    write(
        &leftdir,
        "left.sx",
        concat!(
            "span := import span.span\n",
            "stable := import math.stable\n\n",
            "pub build := (seed: i64) -> span.Span {\n",
            "    var total := stable.sum(stable.Vector { x: seed, y: seed + 1 })\n",
            "    span.Span { start: seed, length: total }\n",
            "}\n",
        ),
    );
    write(
        &rightdir,
        "right.sx",
        concat!(
            "span := import span.span\n",
            "canary := import math.canary\n\n",
            "pub consume := (window: span.Span) -> i64 {\n",
            "    var vec := canary.Vector { x: window.start, y: window.length, z: 5 }\n",
            "    canary.sum(vec)\n",
            "}\n",
        ),
    );
    write(
        &appdir,
        "main.sx",
        concat!(
            "left := import left.left\n",
            "right := import right.right\n",
            "stable := import stable.stable\n",
            "canary := import canary.canary\n",
            "modern := import modern.modern\n\n",
            "pub main := () -> i64 {\n",
            "    var window := left.build(4)\n",
            "    var a := stable.sum(stable.Vector { x: 1, y: 2 })\n",
            "    var b := canary.sum(canary.Vector { x: 1, y: 2, z: 3 })\n",
            "    right.consume(window) + a + b + modern.sum(7)\n",
            "}\n",
        ),
    );
    let mut packages = BTreeMap::new();
    for item in [
        node("perish/span/1.0.0/stable", &sharedir, &[]),
        node("perish/math/1.0.0/stable", &stabledir, &[]),
        node("perish/math/1.0.0/canary", &canarydir, &[]),
        node("perish/math/2.0.0/stable", &vtwodir, &[]),
        node(
            "perish/left/1.0.0/stable",
            &leftdir,
            &[
                ("span", "perish/span/1.0.0/stable"),
                ("math", "perish/math/1.0.0/stable"),
            ],
        ),
        node(
            "perish/right/1.0.0/stable",
            &rightdir,
            &[
                ("span", "perish/span/1.0.0/stable"),
                ("math", "perish/math/1.0.0/canary"),
            ],
        ),
    ] {
        packages.insert(item.coordinate.clone(), item);
    }
    let root = node(
        "perish/app/1.0.0/stable",
        &appdir,
        &[
            ("left", "perish/left/1.0.0/stable"),
            ("right", "perish/right/1.0.0/stable"),
            ("stable", "perish/math/1.0.0/stable"),
            ("canary", "perish/math/1.0.0/canary"),
            ("modern", "perish/math/2.0.0/stable"),
        ],
    );
    let plan = build(&Graph { root, packages }, false).expect("build diamond");
    let symbols: Vec<&str> = plan
        .module
        .functions
        .iter()
        .map(|f| f.symbol.as_str())
        .collect();
    let stable = Coordinate::parse("perish/math/1.0.0/stable")
        .unwrap()
        .namespace();
    let canary = Coordinate::parse("perish/math/1.0.0/canary")
        .unwrap()
        .namespace();
    let modern = Coordinate::parse("perish/math/2.0.0/stable")
        .unwrap()
        .namespace();
    assert!(symbols.contains(&Mangle::symbol(&stable, "stable", "sum").as_str()));
    assert!(symbols.contains(&Mangle::symbol(&canary, "canary", "sum").as_str()));
    assert!(symbols.contains(&Mangle::symbol(&modern, "modern", "sum").as_str()));
    assert!(symbols.contains(&Mangle::symbol(&stable, "util", "factor").as_str()));
    assert!(symbols.contains(&Mangle::symbol(&canary, "util", "factor").as_str()));
    check(&plan.stem, &plan.module, false);
}
