use simplex::{Code, Failure, check, render, scan, verify};
use std::fs;
use std::path::Path;

#[test]
fn repository() {
    let root = Path::new("../../cases");
    let report = verify(root).expect("golden suite clean");
    assert_eq!(report.domains, 8);
    assert_eq!(report.passed, 19);
    assert_eq!(report.rejected, 83);
}

#[test]
fn laws() {
    let sample =
        "route := (code: u16) bucket code {\n    200 => Status.ok\n    _ => Status.fault\n}\n";
    let program = check(sample).expect("valid control sample");
    assert_eq!(render(&program), sample);
    assert!(scan("packet := Packet { tag: 1 }\n").is_ok());
    assert!(scan("wide := port.into<u64>()\n").is_ok());
    assert!(scan("shifted := mask >> 2\n").is_ok());
    assert_eq!(
        scan("// forbidden\nanswer := 1\n").unwrap_err().code,
        Code::Comment
    );
    assert_eq!(scan("user_name := 1\n").unwrap_err().code, Code::Singleword);
    assert_eq!(scan("let port := 1\n").unwrap_err().code, Code::Declaration);
    assert_eq!(scan("count--\n").unwrap_err().code, Code::Assignment);
    assert_eq!(scan("port := 8080;\n").unwrap_err().code, Code::Semicolon);
    assert_eq!(
        scan("wide := (-42).into<i64>()\n").unwrap_err().code,
        Code::Cast
    );
    assert_eq!(
        scan("check(left<right, top>bottom)\n").unwrap_err().code,
        Code::Spacing
    );
    assert_eq!(
        scan("sum := left\n    - right\n").unwrap_err().code,
        Code::Continuation
    );
    assert_eq!(scan("if ready {}\n").unwrap_err().code, Code::If);
    assert_eq!(scan("while ready {}\n").unwrap_err().code, Code::While);
    assert_eq!(
        scan("drain := () -> u64 {\n    return 1\n}\n")
            .unwrap_err()
            .code,
        Code::Return
    );
    assert_eq!(
        scan("grade := (x: i64) bucket {\n    x < 0 => Bound.low\n}\n")
            .unwrap_err()
            .code,
        Code::Bucket
    );
    assert_eq!(scan("_ := 1\n").unwrap_err().code, Code::Discard);
    assert_eq!(
        scan("toggle := (active: bool) switch active {\n    true => Mode.live\n    _ => Mode.idle\n}\n")
            .unwrap_err()
            .code,
        Code::Discard
    );
}

#[test]
fn unlisted() {
    let dir = tempfile::tempdir().expect("tempdir");
    for domain in [
        "async", "borrow", "comptime", "control", "dylib", "lexical", "model", "module",
    ] {
        let sub = dir.path().join(domain);
        fs::create_dir_all(sub.join("reject")).expect("mkdir");
        fs::write(sub.join("suite.toml"), format!("domain = \"{domain}\"\n")).expect("manifest");
    }
    fs::write(dir.path().join("lexical/reject/rogue.sx"), "if ready {}\n").expect("specimen");
    let error = verify(dir.path()).expect_err("unlisted must refuse");
    assert_eq!(
        error,
        Failure::Unlisted {
            domain: "lexical".into(),
            file: "rogue.sx".into(),
        }
    );
}
