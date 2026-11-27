use super::support::{asset, check, node, write};
use simplex_hir::{Graph, Mangle, build};
use simplex_package::Coordinate;
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn staticlink() {
    let scratch = tempdir().expect("tempdir");
    let coredir = scratch.path().join("core");
    let appdir = scratch.path().join("app");
    asset(&coredir, "assets/banner.bin", b"core-asset");
    asset(&appdir, "assets/banner.bin", b"app-asset!");
    write(&coredir, "util.sx", "pub bias := () -> i64 {\n    11\n}\n");
    write(
        &coredir,
        "core.sx",
        concat!(
            "util := import util\n\n",
            "pub banner := embed(\"assets/banner.bin\")\n\n",
            "pub Drop := interface {\n    drop: (self: &var Self)\n}\n\n",
            "pub Metric := interface {\n    score: (self: &Self) -> i64\n}\n\n",
            "pub Boxed<T> := struct {\n    pub value: T\n}\n\n",
            "pub Outcome := enum {\n    empty\n    ready(i64)\n}\n\n",
            "pub Guard := struct {\n    pub code: i64\n}\n\n",
            "Guard : Drop {\n    drop: (self: &var Guard) {\n        self.code = 0\n    }\n}\n\n",
            "Guard : Metric {\n    score: (self: &Guard) -> i64 {\n        self.code + util.bias()\n    }\n}\n\n",
            "pub unpack := <T>(item: Boxed<T>) -> T {\n    item.value\n}\n\n",
            "pub stride := (base: i64, comptime step: i64) -> i64 {\n    base + step\n}\n\n",
            "pub measure := (target: &Metric) -> i64 {\n    target.score()\n}\n\n",
            "pub load := () -> u8 {\n    embed(\"assets/banner.bin\")[5]\n}\n\n",
            "pub compute := async (seed: i64) -> Outcome {\n",
            "    yield()\n",
            "    Outcome.ready(seed + util.bias())\n",
            "}\n",
        ),
    );
    write(&appdir, "util.sx", "pub bias := () -> i64 {\n    100\n}\n");
    write(
        &appdir,
        "main.sx",
        concat!(
            "core := import core.core\n",
            "util := import util\n\n",
            "local := embed(\"assets/banner.bin\")\n\n",
            "pub main := async () -> i64 {\n",
            "    var guard := core.Guard { code: 7 }\n",
            "    var boxed := core.Boxed { value: core.stride(5, 8) }\n",
            "    var got := core.unpack(boxed) + core.measure(&guard) + util.bias()\n",
            "    var byte := core.load().into<usize>()\n",
            "    var outcome := core.compute(got).await\n",
            "    var extra := (core.banner.len() + byte + local.len()).as<i64>()\n",
            "    switch outcome {\n",
            "        core.Outcome.empty => 0\n",
            "        core.Outcome.ready(total) => total + extra + guard.code - guard.code\n",
            "    }\n",
            "}\n",
        ),
    );
    let corecoord = Coordinate::parse("perish/core/1.2.3/stable").unwrap();
    let appcoord = Coordinate::parse("perish/app/0.1.0/stable").unwrap();
    let corepkg = node("perish/core/1.2.3/stable", &coredir, &[]);
    let root = node(
        "perish/app/0.1.0/stable",
        &appdir,
        &[("core", "perish/core/1.2.3/stable")],
    );
    let mut packages = BTreeMap::new();
    packages.insert(corepkg.coordinate.clone(), corepkg);
    let plan = build(&Graph { root, packages }, false).expect("build static");
    assert_eq!(plan.namespace, appcoord.namespace());
    assert_eq!(plan.stem, "app");
    assert!(plan.dynamics.is_empty());
    let expected = Mangle::symbol(&appcoord.namespace(), "main", "main");
    assert_eq!(plan.module.entry.as_deref(), Some(expected.as_str()));
    assert!(plan.module.assets.iter().any(|a| a.bytes == b"core-asset"));
    assert!(plan.module.assets.iter().any(|a| a.bytes == b"app-asset!"));
    let symbols: Vec<&str> = plan
        .module
        .functions
        .iter()
        .map(|f| f.symbol.as_str())
        .collect();
    let corens = corecoord.namespace();
    let appns = appcoord.namespace();
    assert!(symbols.contains(&Mangle::symbol(&corens, "core", "unpack$i64").as_str()));
    assert!(symbols.contains(&Mangle::symbol(&corens, "core", "stride$8").as_str()));
    assert!(symbols.contains(&Mangle::symbol(&corens, "util", "bias").as_str()));
    assert!(symbols.contains(&Mangle::symbol(&appns, "util", "bias").as_str()));
    check(&plan.stem, &plan.module, false);
}
