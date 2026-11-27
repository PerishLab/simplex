use simplex_package::archive::{digest, pack, unpack};
use simplex_package::{Coordinate, Fault, Manifest};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use tar::{Archive, Builder, EntryType, Header};
use tempfile::tempdir;

fn populate(root: &Path) {
    fs::create_dir_all(root.join("sub")).expect("sub");
    fs::create_dir_all(root.join("assets")).expect("assets");
    fs::create_dir_all(root.join(".git")).expect("git");
    fs::create_dir_all(root.join("target")).expect("target");
    fs::create_dir_all(root.join("nested")).expect("nested");
    fs::write(
        root.join("simplex.toml"),
        "schema = \"simplex.package/v1\"\n[library]\nscope = \"perish\"\nname = \"codec\"\nversion = \"1.0.0\"\nchannel = \"stable\"\n[dependencies]\nwire = { workspace = \"*\" }\n",
    )
    .expect("raw toml");
    fs::write(root.join("sub/helper.sx"), "pub value := 7\n").expect("helper");
    fs::write(root.join("codec.sx"), "import sub\npub run := sub.value\n").expect("codec");
    fs::write(root.join("assets/weights.bin"), [1_u8, 2, 3, 4]).expect("asset");
    fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").expect("git head");
    fs::write(root.join(".hidden"), "secret").expect("hidden");
    fs::write(root.join("target/artifact"), "bin").expect("target file");
    fs::write(root.join("seal.toml"), "old seal").expect("seal");
    fs::write(root.join("source.tar"), "old tar").expect("tar");
    fs::write(root.join("nested/simplex.toml"), "nested").expect("nested toml");
    fs::write(root.join("nested/ignored.sx"), "ignored").expect("nested sx");
}

fn inspect(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = Archive::new(Cursor::new(bytes));
    let mut list = Vec::new();
    for item in archive.entries().expect("entries") {
        let mut entry = item.expect("entry");
        let header = entry.header();
        assert_eq!(header.mtime().expect("mtime"), 0);
        assert_eq!(header.uid().expect("uid"), 0);
        assert_eq!(header.gid().expect("gid"), 0);
        assert_eq!(header.mode().expect("mode"), 0o644);
        assert_eq!(header.username().expect("user"), Some(""));
        assert_eq!(header.groupname().expect("group"), Some(""));
        let path = entry.path().expect("path").to_string_lossy().into_owned();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).expect("read");
        list.push((path, buf));
    }
    list
}

fn crafted(raw: &[u8], kind: EntryType, link: Option<&str>) -> Vec<u8> {
    let mut header = Header::new_ustar();
    header.set_entry_type(kind);
    let payload: &[u8] = if kind.is_file() { b"bad" } else { b"" };
    header.set_size(payload.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    let bytes = header.as_mut_bytes();
    bytes[..raw.len()].copy_from_slice(raw);
    if let Some(target) = link {
        let slice = target.as_bytes();
        bytes[157..157 + slice.len()].copy_from_slice(slice);
    }
    header.set_cksum();
    let mut builder = Builder::new(Vec::new());
    builder.append(&header, payload).expect("append");
    builder.into_inner().expect("finish")
}

#[test]
fn determinism() {
    assert_eq!(
        digest(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    let first = tempdir().expect("first");
    populate(first.path());
    let raw = Manifest::load(first.path()).expect("load");
    assert!(matches!(pack(first.path(), &raw), Err(Fault::Manifest(_))));

    let mut members = BTreeMap::new();
    members.insert(
        "wire".into(),
        Coordinate::parse("perish/wire/0.2.0/stable").expect("wire"),
    );
    let rewritten = raw.rewrite(&members).expect("rewrite");
    let (bytes, hash) = pack(first.path(), &rewritten).expect("pack first");
    assert_eq!(digest(&bytes), hash);

    let items = inspect(&bytes);
    let names: Vec<&str> = items.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "simplex.toml",
            "assets/weights.bin",
            "codec.sx",
            "sub/helper.sx"
        ]
    );
    assert_eq!(items[0].1, rewritten.render().into_bytes());

    let second = tempdir().expect("second");
    fs::create_dir_all(second.path().join("assets")).expect("assets");
    fs::create_dir_all(second.path().join("sub")).expect("sub");
    fs::write(second.path().join("assets/weights.bin"), [1_u8, 2, 3, 4]).expect("w");
    fs::write(
        second.path().join("codec.sx"),
        "import sub\npub run := sub.value\n",
    )
    .expect("c");
    fs::write(second.path().join("sub/helper.sx"), "pub value := 7\n").expect("h");
    let (again, secondhash) = pack(second.path(), &rewritten).expect("pack second");
    assert_eq!(bytes, again);
    assert_eq!(hash, secondhash);
}

#[test]
fn extraction() {
    let src = tempdir().expect("src");
    populate(src.path());
    let mut members = BTreeMap::new();
    members.insert(
        "wire".into(),
        Coordinate::parse("perish/wire/0.2.0/stable").expect("wire"),
    );
    let rewritten = Manifest::load(src.path())
        .expect("load")
        .rewrite(&members)
        .expect("rewrite");
    let (bytes, hash) = pack(src.path(), &rewritten).expect("pack");

    let sandbox = tempdir().expect("sandbox");
    let untouchable = sandbox.path().join("untouched");
    let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
    assert!(matches!(
        unpack(&bytes, wrong, &untouchable),
        Err(Fault::Integrity(_))
    ));
    assert!(!untouchable.exists());

    let dest = sandbox.path().join("clean");
    unpack(&bytes, &hash, &dest).expect("unpack");
    let unpacked = Manifest::load(&dest).expect("unpacked manifest");
    assert_eq!(unpacked, rewritten);
    assert_eq!(
        fs::read(dest.join("assets/weights.bin")).expect("asset"),
        vec![1_u8, 2, 3, 4]
    );
    assert!(!dest.join("seal.toml").exists());
    assert!(!dest.join(".hidden").exists());
}

#[test]
fn rejections() {
    let sandbox = tempdir().expect("sandbox");
    let cases = [
        crafted(b"../escape.sx", EntryType::Regular, None),
        crafted(b"sub/../../escape.sx", EntryType::Regular, None),
        crafted(b"/etc/passwd", EntryType::Regular, None),
        crafted(b"sub\\..\\escape.sx", EntryType::Regular, None),
        crafted(b"evil.sx", EntryType::Symlink, Some("/etc/passwd")),
        crafted(b"hard.sx", EntryType::Link, Some("simplex.toml")),
    ];
    for (idx, evil) in cases.iter().enumerate() {
        let hash = digest(evil);
        let target = sandbox.path().join(format!("case{idx}"));
        assert!(
            matches!(unpack(evil, &hash, &target), Err(Fault::Archive(_))),
            "case {idx} should fail"
        );
        assert!(!target.exists(), "case {idx} touched disk");
    }
}
