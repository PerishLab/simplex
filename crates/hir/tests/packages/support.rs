use simplex::Code;
use simplex_hir::{Fault, Graph, Linked, build};
use simplex_ir::{Module, prepare, verify};
use simplex_object::emit;
use simplex_package::Coordinate;
use simplex_target::{Platform, compile};
use std::fs;
use std::path::Path;

pub fn write(root: &Path, rel: &str, raw: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    let tokens = simplex::tokenize(raw).expect("tokenize");
    let parsed = simplex::parser::parse(&tokens).expect("parse");
    let formatted = simplex::render(&parsed);
    fs::write(path, formatted).expect("write");
}

pub fn asset(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, bytes).expect("asset");
}

pub fn node(spec: &str, root: &Path, entry: &str, deps: &[(&str, &str)]) -> Linked {
    let coordinate = Coordinate::parse(spec).expect("coordinate");
    let dependencies = deps
        .iter()
        .map(|(alias, target)| {
            (
                (*alias).to_owned(),
                Coordinate::parse(target).expect("dep coordinate"),
            )
        })
        .collect();
    Linked {
        coordinate,
        root: root.to_path_buf(),
        entry: entry.to_owned(),
        dependencies,
    }
}

pub fn check(stem: &str, module: &Module, shared: bool) {
    verify(module).expect("verify raw ir");
    let mut lowered = module.clone();
    prepare(&mut lowered).expect("prepare ir");
    verify(&lowered).expect("verify prepared ir");
    for platform in [Platform::Linux, Platform::Windows, Platform::Apple] {
        let artifact = compile(platform, stem, &lowered).expect("target compile");
        let bytes = emit(&artifact).expect("object emit");
        assert!(!bytes.is_empty());
        assert_eq!(module.shared, shared);
    }
}

pub fn expectcode(graph: &Graph, expected: Code) {
    match build(graph, false) {
        Err(Fault::Frontend(finding)) => assert_eq!(finding.code, expected),
        other => panic!("expected {expected}, got {other:?}"),
    }
}
