use std::process::Command;

#[test]
fn cli() {
    let out = Command::new(env!("CARGO_BIN_EXE_simplex"))
        .arg("check")
        .arg("../../cases")
        .output()
        .expect("spawn simplex");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("verified 9 domains (38 pass, 140 reject)"));
}
