use simplex_package::{Coordinate, Credentials, Manifest, Seal};
use std::fs;
use tempfile::tempdir;

pub fn sample() -> Credentials {
    Credentials {
        identity: "AKIAIOSFODNN7EXAMPLE".into(),
        secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
        token: None,
    }
}

pub fn bundle(coordinate: &Coordinate, source: &str) -> (Vec<u8>, Seal) {
    let workspace = tempdir().expect("package source tempdir");
    let toml = format!(
        "schema = \"simplex.package/v1\"\n\n[library]\nscope = \"{}\"\nname = \"{}\"\nversion = \"{}\"\nchannel = \"{}\"\nentry = \"{}.sx\"\n",
        coordinate.scope, coordinate.name, coordinate.version, coordinate.channel, coordinate.name
    );
    fs::write(workspace.path().join("simplex.toml"), &toml).expect("write manifest");
    fs::write(
        workspace.path().join(format!("{}.sx", coordinate.name)),
        source,
    )
    .expect("write sx");
    fs::write(workspace.path().join("asset.bin"), b"payload-bytes").expect("write asset");
    let manifest = Manifest::load(workspace.path()).expect("load manifest");
    let (archive, hash) =
        simplex_package::archive::pack(workspace.path(), &manifest).expect("pack");
    let seal = Seal {
        coordinate: coordinate.clone(),
        source: hash,
        api: "a".repeat(64),
        abi: "b".repeat(64),
        dependencies: Vec::new(),
    };
    (archive, seal)
}
