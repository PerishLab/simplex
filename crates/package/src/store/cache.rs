use crate::Fault;
use crate::archive;
use crate::model::Coordinate;
use crate::seal::Seal;
use crate::store::Cached;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn load(cache: &Path, coordinate: &Coordinate) -> Result<Option<Cached>, Fault> {
    let folder = directory(cache, coordinate);
    let metadata = folder.join("seal.toml");
    let tarball = folder.join("source.tar");
    let sealed = metadata.exists();
    let packed = tarball.exists();
    if !sealed && !packed {
        return Ok(None);
    }
    if !sealed || !packed {
        return Err(Fault::Integrity(format!(
            "incomplete cached package: {coordinate}"
        )));
    }
    let text = fs::read_to_string(&metadata).map_err(|err| Fault::Io(err.to_string()))?;
    let seal = Seal::parse(&text)
        .map_err(|err| Fault::Integrity(format!("corrupted cached seal: {err}")))?;
    if seal.coordinate != *coordinate {
        return Err(Fault::Integrity(format!(
            "cached seal coordinate mismatch: expected {coordinate}, got {}",
            seal.coordinate
        )));
    }
    let bytes = fs::read(&tarball).map_err(|err| Fault::Io(err.to_string()))?;
    let actual = archive::digest(&bytes);
    if actual != seal.source {
        return Err(Fault::Integrity(format!(
            "cached archive digest mismatch: expected {}, got {actual}",
            seal.source
        )));
    }
    let unpacked = folder.join("unpacked");
    if !unpacked.join("simplex.toml").exists() {
        refresh(&unpacked, &bytes, &seal.source)?;
    }
    Ok(Some(Cached {
        coordinate: coordinate.clone(),
        root: unpacked,
        archive: bytes,
        seal,
    }))
}

pub(crate) fn save(cache: &Path, bytes: &[u8], seal: &Seal) -> Result<PathBuf, Fault> {
    let folder = directory(cache, &seal.coordinate);
    let unpacked = folder.join("unpacked");
    refresh(&unpacked, bytes, &seal.source)?;
    fs::write(folder.join("source.tar"), bytes).map_err(|err| Fault::Io(err.to_string()))?;
    fs::write(folder.join("seal.toml"), seal.render()).map_err(|err| Fault::Io(err.to_string()))?;
    Ok(unpacked)
}

fn directory(cache: &Path, coordinate: &Coordinate) -> PathBuf {
    cache
        .join(&coordinate.scope)
        .join(&coordinate.name)
        .join(coordinate.version.to_string())
        .join(&coordinate.channel)
}

fn refresh(unpacked: &Path, bytes: &[u8], expected: &str) -> Result<(), Fault> {
    if unpacked.exists() {
        fs::remove_dir_all(unpacked).map_err(|err| Fault::Io(err.to_string()))?;
    }
    archive::unpack(bytes, expected, unpacked)
}
