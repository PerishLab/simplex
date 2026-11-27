use simplex_package::{
    Coordinate, Dependency, Fault, LEMMA, Manifest, Version, WORKSPACE, Workspace,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn seed(folder: &Path, name: &str, extra: &str) {
    fs::create_dir_all(folder).expect("mkdir");
    let body = format!(
        "schema = \"{LEMMA}\"\n\n[lemma]\nscope = \"perish\"\nname = \"{name}\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n{extra}"
    );
    fs::write(folder.join("simplex.toml"), body).expect("write manifest");
    fs::write(folder.join("parse.sx"), "pub answer := 42\n").expect("write sx");
}

#[test]
fn lemma() {
    let text = r#"
schema = "simplex.lemma/v1"

[lemma]
scope = "perish"
name = "codec.json"
version = "1.2.0"
channel = "stable"

[dependencies]
math = { scope = "perish", name = "math", version = "0.4.1", channel = "stable" }
text = { workspace = "codec.text" }
wire = { workspace = "*" }
"#;
    let parsed = Manifest::parse(text).expect("valid manifest");
    assert_eq!(parsed.lemma.to_string(), "perish/codec.json/1.2.0/stable");
    assert_eq!(parsed.lemma.leaf(), "json");
    assert_eq!(parsed.dependencies.len(), 3);
    let math = Coordinate::parse("perish/math/0.4.1/stable").expect("math coord");
    assert_eq!(
        parsed.dependencies.get("math"),
        Some(&Dependency::Exact(math))
    );
    assert_eq!(
        parsed.dependencies.get("text"),
        Some(&Dependency::Workspace(Some(String::from("codec.text"))))
    );
    assert_eq!(
        parsed.dependencies.get("wire"),
        Some(&Dependency::Workspace(None))
    );
    let rendered = parsed.render();
    assert!(rendered.starts_with(&format!("schema = \"{LEMMA}\"\n")));
    assert!(!rendered.contains("entry"));
    assert_eq!(Manifest::parse(&rendered).expect("roundtrip"), parsed);
}

#[test]
fn defaults() {
    let text = "schema = \"simplex.lemma/v1\"\n[lemma]\nscope = \"perish\"\nname = \"wire\"\nversion = \"0.1.0\"\nchannel = \"canary\"\n";
    let parsed = Manifest::parse(text).expect("minimal manifest");
    assert!(parsed.dependencies.is_empty());
    assert_eq!(
        parsed.lemma.version,
        Version {
            major: 0,
            minor: 1,
            patch: 0
        }
    );
    assert_eq!(Manifest::parse(&parsed.render()).expect("reparsed"), parsed);
}

#[test]
fn rejections() {
    let legacy = "schema = \"simplex.package/v1\"\n[library]\nscope = \"perish\"\nname = \"wire\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n";
    assert!(matches!(Manifest::parse(legacy), Err(Fault::Manifest(_))));
    let table = "schema = \"simplex.lemma/v1\"\n[library]\nscope = \"perish\"\nname = \"wire\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n";
    assert!(matches!(Manifest::parse(table), Err(Fault::Manifest(_))));
    let entry = "schema = \"simplex.lemma/v1\"\n[lemma]\nscope = \"perish\"\nname = \"wire\"\nversion = \"1.0.0\"\nchannel = \"stable\"\nentry = \"wire.sx\"\n";
    assert!(matches!(Manifest::parse(entry), Err(Fault::Manifest(_))));
    let topentry = "schema = \"simplex.lemma/v1\"\nentry = \"wire.sx\"\n[lemma]\nscope = \"perish\"\nname = \"wire\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n";
    assert!(matches!(Manifest::parse(topentry), Err(Fault::Manifest(_))));
    let wrong = "schema = \"simplex.workspace/v1\"\nmembers = [\"wire\"]\n";
    assert!(matches!(Manifest::parse(wrong), Err(Fault::Manifest(_))));
    let missing =
        "[lemma]\nscope = \"perish\"\nname = \"wire\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n";
    assert!(matches!(Manifest::parse(missing), Err(Fault::Manifest(_))));
    let extra = "schema = \"simplex.lemma/v1\"\nextra = true\n[lemma]\nscope = \"perish\"\nname = \"wire\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n";
    assert!(matches!(Manifest::parse(extra), Err(Fault::Manifest(_))));
    for bad in ["a.b.c", ".json", "codec.", "a_b", "Codec"] {
        let doc = format!(
            "schema = \"simplex.lemma/v1\"\n[lemma]\nscope = \"perish\"\nname = \"{bad}\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n"
        );
        assert!(
            matches!(Manifest::parse(&doc), Err(Fault::Coordinate(_))),
            "accepted {bad}"
        );
    }
}

#[test]
fn dependencies() {
    let head = "schema = \"simplex.lemma/v1\"\n[lemma]\nscope = \"perish\"\nname = \"app\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n[dependencies]\n";
    for tail in [
        "math = { scope = \"perish\", name = \"math\", version = \"^1.0.0\", channel = \"stable\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \"~1.0.0\", channel = \"stable\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \">=1.0.0\", channel = \"stable\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \"*\", channel = \"stable\" }\n",
    ] {
        assert!(
            matches!(
                Manifest::parse(&format!("{head}{tail}")),
                Err(Fault::Version(_))
            ),
            "expected version fault on {tail}"
        );
    }
    for tail in [
        "math = \"1.0.0\"\n",
        "math = { scope = \"perish\", name = \"math\", version = \"1.0.0\" }\n",
        "math = { scope = \"perish\", name = \"math\", version = \"1.0.0\", channel = \"stable\", optional = true }\n",
        "math = { workspace = true }\n",
        "math = { workspace = \"^1.0\" }\n",
        "math = { workspace = \"1.0.0\" }\n",
        "math = { workspace = \"\" }\n",
        "math = { workspace = \"a.b.c\" }\n",
        "math = { workspace = \".json\" }\n",
        "math = { workspace = \"codec.\" }\n",
        "math = { workspace = \"*\", channel = \"stable\" }\n",
        "my_math = { workspace = \"*\" }\n",
        "Math = { workspace = \"*\" }\n",
    ] {
        assert!(
            matches!(
                Manifest::parse(&format!("{head}{tail}")),
                Err(Fault::Manifest(_))
            ),
            "expected manifest fault on {tail}"
        );
    }
}

#[test]
fn discovery() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).expect("repo");
    fs::write(
        root.join("simplex.toml"),
        format!(
            "schema = \"{WORKSPACE}\"\nmembers = [\"lemmas/wire\", \"lemmas/codec/json\", \"lemmas/app\"]\n"
        ),
    )
    .expect("workspace toml");
    seed(&root.join("lemmas/wire"), "wire", "");
    seed(&root.join("lemmas/codec/json"), "codec.json", "");
    seed(
        &root.join("lemmas/app"),
        "app",
        "\n[dependencies]\nwire = { workspace = \"*\" }\njson = { workspace = \"codec.json\" }\n",
    );

    let loaded = Workspace::load(&root).expect("workspace load");
    assert_eq!(loaded.root, root);
    assert_eq!(loaded.members.len(), 3);
    assert_eq!(
        Workspace::locate(&root.join("lemmas/codec/json")).expect("dir"),
        Some(loaded.clone())
    );
    assert_eq!(
        Workspace::locate(&root.join("lemmas/codec/json/parse.sx")).expect("file"),
        Some(loaded.clone())
    );
    let outside = tempdir().expect("outside");
    assert_eq!(Workspace::locate(outside.path()).expect("outside"), None);

    let coords = loaded.coordinates();
    let wire = Coordinate::parse("perish/wire/1.0.0/stable").expect("wire");
    let json = Coordinate::parse("perish/codec.json/1.0.0/stable").expect("json");
    assert_eq!(coords.get("wire"), Some(&wire));
    assert_eq!(coords.get("codec.json"), Some(&json));
    let (_, app) = loaded.members.get("app").expect("app member");
    let rewritten = app.rewrite(&coords).expect("rewrite");
    assert_eq!(
        rewritten.dependencies.get("wire"),
        Some(&Dependency::Exact(wire))
    );
    assert_eq!(
        rewritten.dependencies.get("json"),
        Some(&Dependency::Exact(json))
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
        "schema = \"simplex.lemma/v1\"\n[lemma]\nscope = \"perish\"\nname = \"app\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n[dependencies]\nmissing = { workspace = \"*\" }\n",
    )
    .expect("orphan");
    assert!(matches!(
        orphan.rewrite(&BTreeMap::new()),
        Err(Fault::Workspace(_))
    ));
    let named = Manifest::parse(
        "schema = \"simplex.lemma/v1\"\n[lemma]\nscope = \"perish\"\nname = \"app\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n[dependencies]\njson = { workspace = \"codec.json\" }\n",
    )
    .expect("named");
    assert!(matches!(
        named.rewrite(&BTreeMap::new()),
        Err(Fault::Workspace(_))
    ));
}
