mod armor;
mod late;
mod memory;
mod runner;
mod standalone;

use std::env;

fn enabled(target: &str) -> bool {
    let Some(raw) = env::var_os("SIMPLEX_REMOTE") else {
        return false;
    };
    let text = raw.to_string_lossy();
    match text.trim() {
        "" | "0" => false,
        "windows" => target == "windows",
        "apple" => target == "apple",
        _ => true,
    }
}

#[test]
fn corpus() {
    let workspace = runner::Workspace::open();
    let mut items = standalone::stage(&workspace);
    items.extend(memory::stage(&workspace));
    items.extend(late::stage(&workspace));
    assert_eq!(items.len(), 14);
    workspace.stage("codec.sx", &late::library(10, 3));
    workspace.stage("main.sx", late::caller());
    workspace.stage("codec.sx", &late::library(30, 11));
    assert_eq!(armor::wide("ok"), "bwBrAA==");
    let parsed = armor::parse("cleanup:16:343321\n");
    assert_eq!(parsed[0].output, b"43!");
}

#[test]
fn windows() {
    if !enabled("windows") {
        return;
    }
    let baseline = runner::linux();
    let stream = runner::prepare("windows-x86_64", ".exe", "codec.dll");
    let remote = runner::windows(&stream);
    assert_eq!(remote, baseline);
}

#[test]
fn apple() {
    if !enabled("apple") {
        return;
    }
    let baseline = runner::linux();
    let stream = runner::prepare("darwin-aarch64", "", "codec.dylib");
    let remote = runner::apple(&stream);
    assert_eq!(remote, baseline);
}
