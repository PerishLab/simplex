mod collision;
mod manifest;
mod module;
mod surface;

use crate::harness::{Client, Outcome, Sandbox};

pub const UNIT: &str = "pub unit := () -> u64 { 7 }\n";

pub fn attempt(sandbox: &Sandbox, client: &Client, folder: &str) -> Vec<Outcome> {
    let library = sandbox.join(&format!("{folder}.so"));
    let output = library.to_str().expect("library path");
    let outcomes = vec![
        sandbox.run(&["seal", folder], Some(client)),
        sandbox.run(&["publish", folder], Some(client)),
        sandbox.run(&["build", folder, "--dylib", "-o", output], Some(client)),
    ];
    assert_eq!(outcomes[2].passed, library.exists());
    outcomes
}

pub fn refuse(sandbox: &Sandbox, client: &Client, folder: &str) {
    for outcome in attempt(sandbox, client, folder) {
        assert!(!outcome.passed, "{folder} accepted");
        assert_ne!(outcome.code, 0);
    }
}
