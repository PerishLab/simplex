use simplex::Code;
use simplex_hir::{Fault, lower};
use std::fs;
use std::path::Path;

#[test]
fn specimens() {
    let root = Path::new("../../cases");
    let mut total = 0usize;
    for domain in [
        "async", "borrow", "comptime", "control", "dylib", "lexical", "meta", "model", "module",
    ] {
        let dir = root.join(domain).join("pass");
        let mut files: Vec<_> = fs::read_dir(&dir)
            .expect("pass dir")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("sx"))
            .collect();
        files.sort();
        for path in files {
            let shared = domain == "dylib" && path.file_stem().unwrap() != "plugin";
            let unit = lower("app", &path, shared).expect("pass specimen lowers");
            assert_eq!(unit.namespace, "app");
            assert_eq!(unit.shared, shared);
            total += 1;
        }
    }
    assert_eq!(total, 38);
}

#[test]
fn rejections() {
    let missing = Path::new("../../cases/missing.sx");
    assert!(matches!(lower("app", missing, false), Err(Fault::Io(_))));

    let cycle = Path::new("../../cases/module/reject/cyclea.sx");
    assert_eq!(
        lower("app", cycle, false),
        Err(Fault::Frontend(simplex::Finding {
            code: Code::Cycle,
            line: 1,
            column: 1,
        }))
    );

    let unawaited = Path::new("../../cases/async/reject/unawaited.sx");
    match lower("app", unawaited, false) {
        Err(Fault::Frontend(f)) => assert_eq!(f.code, Code::Detached),
        other => panic!("expected detached fault, got {other:?}"),
    }
}
