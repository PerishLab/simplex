use super::support::{check, node, write};
use simplex_hir::{Graph, build};
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
        "math.sx",
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
        "math.sx",
        concat!(
            "util := import util\n\n",
            "pub Vector := struct {\n    pub x: i64\n    pub y: i64\n    pub z: i64\n}\n\n",
            "pub sum := (v: Vector) -> i64 {\n    (v.x + v.y + v.z) * util.factor()\n}\n",
        ),
    );
    write(
        &vtwodir,
        "math.sx",
        "pub sum := (seed: i64) -> i64 {\n    seed * 10\n}\n",
    );
    write(
        &leftdir,
        "left.sx",
        concat!(
            "span := import span\n",
            "math := import math\n\n",
            "pub build := (seed: i64) -> span.Span {\n",
            "    var total := math.sum(math.Vector { x: seed, y: seed + 1 })\n",
            "    span.Span { start: seed, length: total }\n",
            "}\n",
        ),
    );
    write(
        &rightdir,
        "right.sx",
        concat!(
            "span := import span\n",
            "math := import math\n\n",
            "pub consume := (window: span.Span) -> i64 {\n",
            "    var vec := math.Vector { x: window.start, y: window.length, z: 5 }\n",
            "    math.sum(vec)\n",
            "}\n",
        ),
    );
    write(
        &appdir,
        "app.sx",
        concat!(
            "left := import left\n",
            "right := import right\n",
            "stable := import stable\n",
            "canary := import canary\n",
            "modern := import modern\n\n",
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
        node("perish/span/1.0.0/stable", &sharedir, "span.sx", &[]),
        node("perish/math/1.0.0/stable", &stabledir, "math.sx", &[]),
        node("perish/math/1.0.0/canary", &canarydir, "math.sx", &[]),
        node("perish/math/2.0.0/stable", &vtwodir, "math.sx", &[]),
        node(
            "perish/left/1.0.0/stable",
            &leftdir,
            "left.sx",
            &[
                ("span", "perish/span/1.0.0/stable"),
                ("math", "perish/math/1.0.0/stable"),
            ],
        ),
        node(
            "perish/right/1.0.0/stable",
            &rightdir,
            "right.sx",
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
        "app.sx",
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
    assert!(symbols.contains(&"sx$perish$math$1_0_0$stable$math$sum"));
    assert!(symbols.contains(&"sx$perish$math$1_0_0$canary$math$sum"));
    assert!(symbols.contains(&"sx$perish$math$2_0_0$stable$math$sum"));
    assert!(symbols.contains(&"sx$perish$math$1_0_0$stable$util$factor"));
    assert!(symbols.contains(&"sx$perish$math$1_0_0$canary$util$factor"));
    check(&plan.stem, &plan.module, false);
}
