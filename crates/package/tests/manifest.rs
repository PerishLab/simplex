use simplex_package::{
    Coordinate, Dependency, Fault, Manifest, PACKAGE, Version, WORKSPACE, Workspace,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn seed(folder: &Path, name: &str, extra: &str) {
    fs::create_dir_all(folder).expect("mkdir");
    let body = format!(
        "schema = \"{PACKAGE}\"\n\n[library]\nscope = \"perish\"\nname = \"{name}\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n{extra}"
    );
    fs::write(folder.join("simplex.toml"), body).expect("write manifest");
    fs::write(folder.join(format!("{name}.sx")), "pub answer := 42\n").expect("write sx");
}

#[test]
fn package() {
    let text = r#"
schema = "simplex.package/v1"

[library]
scope = "perish"
name = "codec"
version = "1.2.0"
channel = "stable"
entry = "src/codec.sx"

[dependencies]
math = { scope = "perish", name = "math", version = "0.4.1", channel = "stable" }
wire = { workspace = "*" }
"#;
    let parsed = Manifest::parse(text).expect("valid manifest");
    assert_eq!(parsed.library.to_string(), "perish/codec/1.2.0/stable");
    assert_eq!(parsed.entry, "src/codec.sx");
    assert_eq!(parsed.dependencies.len(), 2);
    let math = Coordinate::parse("perish/math/0.4.1/stable").expect("math coord");
    assert_eq!(
        parsed.dependencies.get("math"),
        Some(&Dependency::Exact(math))
    );
    assert_eq!(
        parsed.dependencies.get("wire"),
        Some(&Dependency::Workspace)
    );
    let rendered = parsed.render();
    assert!(rendered.starts_with(&format!("schema = \"{PACKAGE}\"\n")));
    assert_eq!(Manifest::parse(&rendered).expect("roundtrip"), parsed);
}

#[test]
fn defaults() {
    let text = "schema = \"simplex.package/v1\"\n[library]\nscope = \"perish\"\nname = \"wire\"\nversion = \"0.1.0\"\nchannel = \"canary\"\n";
    let parsed = Manifest::parse(text).expect("minimal manifest");
    assert_eq!(parsed.entry, "wire.sx");
    assert!(parsed.dependencies.is_empty());
    assert_eq!(
        parsed.library.version,
        Version {
            major: 0,
            minor: 1,
            patch: 0
        }
    );
    assert_eq!(Manifest::parse(&parsed.render()).expect("reparsed"), parsed);
}

#[test]
fn schema() {
    let wrong = "schema = \"simplex.workspace/v1\"\nmembers = [\"wire\"]\n";
    assert!(matches!(Manifest::parse(wrong), Err(Fault::Manifest(_))));
    let missing = "[library]\nscope = \"perish\"\nname = \"wire\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n";
    assert!(matches!(Manifest::parse(missing), Err(Fault::Manifest(_))));
    let extra = "schema = \"simplex.package/v1\"\nextra = true\n[library]\nscope = \"perish\"\nname = \"wire\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n";
    assert!(matches!(Manifest::parse(extra), Err(Fault::Manifest(_))));
}

#[test]
fn entries() {
    for bad in [
        "codec.rs",
        ".sx",
        "/codec.sx",
        "../codec.sx",
        "src/../codec.sx",
        "src//codec.sx",
        "./codec.sx",
        "codec\\main.sx",
        "mod.sx",
        "index.sx",
        "lib.sx",
        "src/mod.sx",
        "src/index.sx",
        "src/lib.sx",
    ] {
        let doc = format!(
            "schema = \"simplex.package/v1\"\n[library]\nscope = \"perish\"\nname = \"codec\"\nversion = \"1.0.0\"\nchannel = \"stable\"\nentry = \"{bad}\"\n"
        );
        assert!(Manifest::parse(&doc).is_err(), "accepted {bad}");
    }
    let barrel = "schema = \"simplex.package/v1\"\n[library]\nscope = \"perish\"\nname = \"lib\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n";
    assert!(matches!(Manifest::parse(barrel), Err(Fault::Manifest(_))));
}

#[test]
fn dependencies() {
    let head = "schema = \"simplex.package/v1\"\n[library]\nscope = \"perish\"\nname = \"app\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n[dependencies]\n";
    for tail in [
        "math = \"1.0.0\"\n",
        "math = { scope = \"perish\", name = \"math\", version = \"^1.0.0\", channel = \"stable\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \"~1.0.0\", channel = \"stable\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \">=1.0.0\", channel = \"stable\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \"*\", channel = \"stable\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \"1.0.0\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \"1.0.0\", channel = \"stable\", optional = true }\n",
        "math = { workspace = true }\n",
        "math = { workspace = \"^1.0\" }\n",
        "math = { workspace = \"*\", channel = \"stable\" }\n",
        "my_math = { workspace = \"*\" }\n",
        "Math = { workspace = \"*\" }\n",
    ] {
        assert!(Manifest::parse(&format!("{head}{tail}")).is_err());
    }
}

#[test]
fn discovery() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).expect("repo");
    fs::write(
        root.join("simplex.toml"),
        format!("schema = \"{WORKSPACE}\"\nmembers = [\"wire\", \"crates/codec\"]\n"),
    )
    .expect("workspace toml");
    seed(&root.join("wire"), "wire", "");
    seed(
        &root.join("crates/codec"),
        "codec",
        "\n[dependencies]\nwire = { workspace = \"*\" }\n",
    );

    let loaded = Workspace::load(&root).expect("workspace load");
    assert_eq!(loaded.root, root);
    assert_eq!(loaded.members.len(), 2);
    assert_eq!(
        Workspace::locate(&root.join("crates/codec")).expect("dir"),
        Some(loaded.clone())
    );
    assert_eq!(
        Workspace::locate(&root.join("crates/codec/codec.sx")).expect("file"),
        Some(loaded.clone())
    );
    let outside = tempdir().expect("outside");
    assert_eq!(Workspace::locate(outside.path()).expect("outside"), None);

    let coords = loaded.coordinates();
    let expected = Coordinate::parse("perish/wire/1.0.0/stable").expect("wire");
    assert_eq!(coords.get("wire"), Some(&expected));
    let (_, codec) = loaded.members.get("codec").expect("codec member");
    let rewritten = codec.rewrite(&coords).expect("rewrite");
    assert_eq!(
        rewritten.dependencies.get("wire"),
        Some(&Dependency::Exact(expected))
    );
}

#[test]
fn lockfiles() {
    let temp = tempdir().expect("tempdir");
    let pkg = temp.path().join("pkg");
    seed(&pkg, "wire", "");
    fs::write(pkg.join("simplex.lock"), "").expect("lock");
    assert_eq!(
        Manifest::load(&pkg),
        Err(Fault::Manifest("lockfile forbidden: simplex.lock".into()))
    );

    let ws = temp.path().join("ws");
    fs::create_dir_all(&ws).expect("ws");
    fs::write(
        ws.join("simplex.toml"),
        "schema = \"simplex.workspace/v1\"\nmembers = [\"a\"]\n",
    )
    .expect("ws toml");
    seed(&ws.join("a"), "a", "");
    fs::write(ws.join("simplex.lock"), "").expect("ws lock");
    assert_eq!(
        Workspace::load(&ws),
        Err(Fault::Manifest("lockfile forbidden: simplex.lock".into()))
    );
    fs::remove_file(ws.join("simplex.lock")).expect("rm ws lock");
    fs::write(ws.join("a/simplex.lock"), "").expect("member lock");
    assert_eq!(
        Workspace::load(&ws),
        Err(Fault::Manifest("lockfile forbidden: simplex.lock".into()))
    );
}

#[test]
fn failures() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path();
    seed(&root.join("one"), "dup", "");
    seed(&root.join("two"), "dup", "");
    let dup = "schema = \"simplex.workspace/v1\"\nmembers = [\"one\", \"two\"]\n";
    assert!(matches!(
        Workspace::parse(root, dup),
        Err(Fault::Workspace(_))
    ));
    let same = "schema = \"simplex.workspace/v1\"\nmembers = [\"one\", \"one\"]\n";
    assert!(matches!(
        Workspace::parse(root, same),
        Err(Fault::Workspace(_))
    ));
    for escape in ["../escape", "/abs", "a/../b", ""] {
        let doc = format!("schema = \"simplex.workspace/v1\"\nmembers = [\"{escape}\"]\n");
        assert!(matches!(
            Workspace::parse(root, &doc),
            Err(Fault::Workspace(_))
        ));
    }
    let extra = "schema = \"simplex.workspace/v1\"\nmembers = []\nextra = 1\n";
    assert!(matches!(
        Workspace::parse(root, extra),
        Err(Fault::Workspace(_))
    ));
    let orphan = Manifest::parse(
        "schema = \"simplex.package/v1\"\n[library]\nscope = \"perish\"\nname = \"app\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n[dependencies]\nmissing = { workspace = \"*\" }\n",
    )
    .expect("orphan");
    assert!(matches!(
        orphan.rewrite(&BTreeMap::new()),
        Err(Fault::Workspace(_))
    ));
}
