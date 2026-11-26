use simplex::{Code, Failure, scan, verify};
use std::fs;
use std::path::Path;

#[test]
fn repository() {
    let root = Path::new("../../cases");
    let report = verify(root).expect("golden suite clean");
    assert_eq!(report.domains, 8);
    assert!(report.passed >= 1);
    assert!(report.rejected >= 4);
}

#[test]
fn laws() {
    assert!(scan("packet := Packet { tag: 1 }\n").is_ok());
    assert_eq!(
        scan("// forbidden\nanswer := 1\n").unwrap_err().code,
        Code::Comment
    );
    assert_eq!(
        scan("/* block */\nanswer := 1\n").unwrap_err().code,
        Code::Comment
    );
    assert_eq!(scan("user_name := 1\n").unwrap_err().code, Code::Singleword);
    assert_eq!(scan("userName := 1\n").unwrap_err().code, Code::Singleword);
    assert_eq!(
        scan("PacketHeader := 1\n").unwrap_err().code,
        Code::Singleword
    );
    assert_eq!(scan("if ready {}\n").unwrap_err().code, Code::If);
    assert_eq!(
        scan("answer := \"unterminated\n").unwrap_err().code,
        Code::Literal
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
