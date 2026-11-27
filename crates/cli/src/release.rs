use crate::remote::{Context, locate};
use simplex_package::{
    Contract, Coordinate, Dependency, Fault, Locked, Manifest, Release, Seal, Store, Workspace,
};
use std::fs;
use std::path::Path;

pub struct Options<'a> {
    pub path: &'a Path,
    pub output: Option<&'a Path>,
    pub baseline: Option<&'a str>,
    pub epoch: bool,
    pub context: &'a Context,
}

struct Prepared {
    archive: Vec<u8>,
    contract: Contract,
    seal: Seal,
}

pub fn seal(options: &Options<'_>) -> Result<(), String> {
    let root = locate(options.path);
    let store = options.context.store()?;
    let prepared = prepare(&root, &store, false)?;
    let baseline = match options.baseline {
        Some(raw) => Some(Coordinate::parse(raw).map_err(|error| error.to_string())?),
        None => None,
    };
    let current = Release {
        seal: &prepared.seal,
        contract: &prepared.contract,
    };
    audit(&store, baseline.as_ref(), current, options.epoch)?;
    let rendered = prepared.seal.render();
    if let Some(path) = options.output {
        if let Some(folder) = path.parent()
            && !folder.as_os_str().is_empty()
        {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        fs::write(path, rendered).map_err(|error| error.to_string())?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}

pub fn publish(options: &Options<'_>) -> Result<(), String> {
    let root = locate(options.path);
    let store = options.context.store()?;
    let prepared = prepare(&root, &store, true)?;
    let coordinate = &prepared.seal.coordinate;
    let baseline = match options.baseline {
        Some(raw) => Some(Coordinate::parse(raw).map_err(|error| error.to_string())?),
        None => match store
            .latest(&coordinate.scope, &coordinate.name, &coordinate.channel)
            .map_err(|error| error.to_string())?
        {
            Some(version) if version == coordinate.version => None,
            Some(version) => Some(
                Coordinate::new(
                    &coordinate.scope,
                    &coordinate.name,
                    version,
                    &coordinate.channel,
                )
                .map_err(|error| error.to_string())?,
            ),
            None => None,
        },
    };
    let current = Release {
        seal: &prepared.seal,
        contract: &prepared.contract,
    };
    audit(&store, baseline.as_ref(), current, options.epoch)?;
    store
        .publish(&prepared.archive, &prepared.seal)
        .map_err(|error| error.to_string())?;
    println!("published {}", prepared.seal.coordinate);
    Ok(())
}

fn prepare(root: &Path, store: &Store, strict: bool) -> Result<Prepared, String> {
    let raw = Manifest::load(root).map_err(|error| error.to_string())?;
    let workspace = Workspace::locate(root).map_err(|error| error.to_string())?;
    let manifest = if raw
        .dependencies
        .values()
        .any(|item| matches!(item, Dependency::Workspace(_)))
    {
        let Some(found) = workspace.as_ref() else {
            return Err("workspace manifest required for workspace dependency".into());
        };
        raw.rewrite(&found.coordinates())
            .map_err(|error| error.to_string())?
    } else {
        raw
    };
    let dependencies = lock(&manifest, store, workspace.as_ref(), strict)?;
    let (archive, source) =
        simplex_package::archive::pack(root, &manifest).map_err(|error| error.to_string())?;
    let contract =
        simplex_package::surface::extract(root, &manifest).map_err(|error| error.to_string())?;
    let seal = Seal {
        coordinate: manifest.lemma,
        source,
        api: contract.api.clone(),
        abi: contract.abi.clone(),
        dependencies,
    };
    Ok(Prepared {
        archive,
        contract,
        seal,
    })
}

fn lock(
    manifest: &Manifest,
    store: &Store,
    workspace: Option<&Workspace>,
    strict: bool,
) -> Result<Vec<Locked>, String> {
    let mut locked = Vec::with_capacity(manifest.dependencies.len());
    for (alias, item) in &manifest.dependencies {
        let Dependency::Exact(coordinate) = item else {
            return Err("unrewritten workspace dependency".into());
        };
        let entry = match store.fetch(coordinate) {
            Ok(cached) => Locked {
                alias: alias.clone(),
                coordinate: coordinate.clone(),
                source: cached.seal.source,
                api: cached.seal.api,
                abi: cached.seal.abi,
            },
            Err(Fault::Offline(_) | Fault::Missing(_)) if !strict => {
                local(alias, coordinate, workspace)?
            }
            Err(error) => return Err(error.to_string()),
        };
        locked.push(entry);
    }
    Ok(locked)
}

fn local(
    alias: &str,
    coordinate: &Coordinate,
    workspace: Option<&Workspace>,
) -> Result<Locked, String> {
    let Some(space) = workspace else {
        return Err(format!("missing dependency in cache: {coordinate}"));
    };
    let Some((folder, member)) = space
        .members
        .values()
        .find(|(_, item)| item.lemma == *coordinate)
    else {
        return Err(format!("missing dependency in cache: {coordinate}"));
    };
    let rewritten = member
        .rewrite(&space.coordinates())
        .map_err(|error| error.to_string())?;
    let (_, source) =
        simplex_package::archive::pack(folder, &rewritten).map_err(|error| error.to_string())?;
    let contract =
        simplex_package::surface::extract(folder, &rewritten).map_err(|error| error.to_string())?;
    Ok(Locked {
        alias: alias.to_owned(),
        coordinate: coordinate.clone(),
        source,
        api: contract.api,
        abi: contract.abi,
    })
}

fn audit(
    store: &Store,
    baseline: Option<&Coordinate>,
    current: Release<'_>,
    epoch: bool,
) -> Result<(), String> {
    let Some(coordinate) = baseline else {
        simplex_package::audit::verify(None, current, epoch).map_err(|error| error.to_string())?;
        return Ok(());
    };
    let cached = store.fetch(coordinate).map_err(|error| error.to_string())?;
    let manifest = Manifest::load(&cached.root).map_err(|error| error.to_string())?;
    let prior = simplex_package::surface::extract(&cached.root, &manifest)
        .map_err(|error| error.to_string())?;
    let previous = Release {
        seal: &cached.seal,
        contract: &prior,
    };
    simplex_package::audit::verify(Some(previous), current, epoch)
        .map_err(|error| error.to_string())?;
    Ok(())
}
