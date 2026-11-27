use crate::manifest::forbid;
use crate::{Dependency, Fault, Manifest};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use tar::{Archive, Builder, EntryType, Header};

pub fn digest(bytes: &[u8]) -> String {
    let sum = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in sum {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn pack(root: &Path, manifest: &Manifest) -> Result<(Vec<u8>, String), Fault> {
    if manifest
        .dependencies
        .values()
        .any(|item| matches!(item, Dependency::Workspace))
    {
        return Err(Fault::Manifest(
            "unrewritten workspace dependency cannot be packed".into(),
        ));
    }
    forbid(root)?;
    let mut files = BTreeMap::new();
    collect(root, root, "", &mut files)?;
    let mut builder = Builder::new(Vec::new());
    let rendered = manifest.render();
    append(&mut builder, "simplex.toml", rendered.as_bytes())?;
    for (rel, full) in files {
        let data =
            fs::read(&full).map_err(|err| Fault::Io(format!("{}: {err}", full.display())))?;
        append(&mut builder, &rel, &data)?;
    }
    let bytes = builder
        .into_inner()
        .map_err(|err| Fault::Archive(err.to_string()))?;
    let hash = digest(&bytes);
    Ok((bytes, hash))
}

pub fn unpack(bytes: &[u8], expected: &str, destination: &Path) -> Result<(), Fault> {
    let actual = digest(bytes);
    if actual != expected {
        return Err(Fault::Integrity(format!(
            "archive digest mismatch: expected {expected}, got {actual}"
        )));
    }
    let staged = inspect(bytes)?;
    fs::create_dir_all(destination)
        .map_err(|err| Fault::Io(format!("{}: {err}", destination.display())))?;
    for (rel, payload) in staged {
        write(destination, &rel, payload.as_deref())?;
    }
    Ok(())
}

fn collect(
    root: &Path,
    current: &Path,
    prefix: &str,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<(), Fault> {
    let entries =
        fs::read_dir(current).map_err(|err| Fault::Io(format!("{}: {err}", current.display())))?;
    for item in entries {
        let entry = item.map_err(|err| Fault::Io(err.to_string()))?;
        let Ok(name) = entry.file_name().into_string() else {
            return Err(Fault::Archive("non-utf8 path in package".into()));
        };
        if name == "simplex.lock" {
            return Err(Fault::Manifest("lockfile forbidden: simplex.lock".into()));
        }
        if skip(current == root, &name) {
            continue;
        }
        let full = entry.path();
        let rel = join(prefix, &name);
        let kind = entry
            .file_type()
            .map_err(|err| Fault::Io(format!("{}: {err}", full.display())))?;
        if kind.is_symlink() {
            return Err(Fault::Archive(format!("symlink forbidden: {rel}")));
        }
        if kind.is_dir() {
            if full.join("simplex.toml").symlink_metadata().is_ok() {
                continue;
            }
            collect(root, &full, &rel, files)?;
        } else if kind.is_file() {
            files.insert(rel, full);
        }
    }
    Ok(())
}

fn skip(top: bool, name: &str) -> bool {
    if name.starts_with('.') || matches!(name, "target" | "seal.toml" | "source.tar") {
        return true;
    }
    top && name == "simplex.toml"
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    }
}

fn append(builder: &mut Builder<Vec<u8>>, path: &str, data: &[u8]) -> Result<(), Fault> {
    let mut header = Header::new_ustar();
    header.set_entry_type(EntryType::Regular);
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header
        .set_username("")
        .map_err(|err| Fault::Archive(err.to_string()))?;
    header
        .set_groupname("")
        .map_err(|err| Fault::Archive(err.to_string()))?;
    builder
        .append_data(&mut header, path, data)
        .map_err(|err| Fault::Archive(err.to_string()))
}

type Staged = (PathBuf, Option<Vec<u8>>);

fn inspect(bytes: &[u8]) -> Result<Vec<Staged>, Fault> {
    let mut archive = Archive::new(Cursor::new(bytes));
    let iter = archive
        .entries()
        .map_err(|err| Fault::Archive(err.to_string()))?;
    let mut seen = BTreeSet::new();
    let mut staged = Vec::new();
    for item in iter {
        let mut entry = item.map_err(|err| Fault::Archive(err.to_string()))?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(Fault::Archive("link entry forbidden in archive".into()));
        }
        if !kind.is_file() && !kind.is_dir() {
            return Err(Fault::Archive("unsupported archive entry".into()));
        }
        let raw = entry.path_bytes().into_owned();
        let rel = sanitize(&raw, kind.is_dir())?;
        if kind.is_file() {
            if !seen.insert(rel.clone()) {
                return Err(Fault::Archive(format!(
                    "duplicate archive entry: {}",
                    rel.display()
                )));
            }
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|err| Fault::Archive(err.to_string()))?;
            staged.push((rel, Some(buf)));
        } else {
            staged.push((rel, None));
        }
    }
    Ok(staged)
}

fn sanitize(raw: &[u8], dir: bool) -> Result<PathBuf, Fault> {
    let Ok(text) = std::str::from_utf8(raw) else {
        return Err(Fault::Archive("non-utf8 archive entry".into()));
    };
    let trimmed = if dir {
        text.strip_suffix('/').unwrap_or(text)
    } else {
        text
    };
    if trimmed.is_empty() || text.starts_with('/') || trimmed.ends_with('/') {
        return Err(Fault::Archive(format!("unsafe archive path: {text}")));
    }
    if trimmed.bytes().any(|byte| matches!(byte, b'\\' | b':' | 0)) {
        return Err(Fault::Archive(format!("unsafe archive path: {text}")));
    }
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(Fault::Archive(format!("unsafe archive path: {text}")));
        }
    }
    let path = PathBuf::from(trimmed);
    if !path
        .components()
        .all(|comp| matches!(comp, Component::Normal(_)))
    {
        return Err(Fault::Archive(format!("unsafe archive path: {text}")));
    }
    Ok(path)
}

fn write(destination: &Path, rel: &Path, payload: Option<&[u8]>) -> Result<(), Fault> {
    let target = destination.join(rel);
    let Some(bytes) = payload else {
        return fs::create_dir_all(&target)
            .map_err(|err| Fault::Io(format!("{}: {err}", target.display())));
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| Fault::Io(format!("{}: {err}", parent.display())))?;
    }
    fs::write(&target, bytes).map_err(|err| Fault::Io(format!("{}: {err}", target.display())))
}
