use crate::remote::{Context, locate, parent};
use simplex_hir::{Graph, Linked};
use simplex_package::{Coordinate, Dependency, Manifest, Store, Workspace};
use simplex_target::Platform;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Request<'a> {
    pub entry: &'a Path,
    pub output: Option<&'a Path>,
    pub target: Option<&'a str>,
    pub shared: bool,
    pub context: &'a Context,
}

struct Loader<'a> {
    store: &'a Store,
    workspace: Option<&'a Workspace>,
}

pub fn compile(request: &Request<'_>) -> Result<(), String> {
    let platform = resolve(request.target)?;
    if let Some(root) = detect(request.entry)? {
        return package(request, platform, &root);
    }
    standalone(request, platform)
}

fn detect(entry: &Path) -> Result<Option<PathBuf>, String> {
    if entry.is_dir() {
        return Ok(Some(entry.to_path_buf()));
    }
    if entry.file_name().and_then(|part| part.to_str()) == Some("simplex.toml") {
        return Ok(Some(parent(entry)));
    }
    let folder = locate(entry);
    let config = folder.join("simplex.toml");
    if !config.is_file() {
        if folder.join("simplex.lock").exists() {
            return Err("lockfile forbidden: simplex.lock".into());
        }
        return Ok(None);
    }
    let text = fs::read_to_string(&config).map_err(|error| error.to_string())?;
    if Workspace::parse(&folder, &text).is_ok() {
        if folder.join("simplex.lock").exists() {
            return Err("lockfile forbidden: simplex.lock".into());
        }
        return Ok(None);
    }
    Ok(Some(folder))
}

fn standalone(request: &Request<'_>, platform: Platform) -> Result<(), String> {
    let name = stem(request.entry);
    let path = request.output.map_or_else(
        || destination(&name, platform, request.shared),
        Path::to_path_buf,
    );
    let mut module = simplex_hir::translate("app", request.entry, request.shared)
        .map_err(|error| error.to_string())?;
    simplex_ir::prepare(&mut module).map_err(|error| error.to_string())?;
    let artifact =
        simplex_target::compile(platform, &name, &module).map_err(|error| error.to_string())?;
    let bytes = simplex_object::emit(&artifact).map_err(|error| error.to_string())?;
    write(&path, &bytes, request.shared)
}

fn package(request: &Request<'_>, platform: Platform, root: &Path) -> Result<(), String> {
    let store = request.context.store()?;
    let mut manifest = Manifest::load(root).map_err(|error| error.to_string())?;
    if request.entry.extension().and_then(|part| part.to_str()) == Some("sx")
        && let Some(file) = request.entry.file_name().and_then(|part| part.to_str())
    {
        manifest.entry = file.to_owned();
    }
    let workspace = Workspace::locate(root).map_err(|error| error.to_string())?;
    let loader = Loader {
        store: &store,
        workspace: workspace.as_ref(),
    };
    let graph = loader.graph(root, &manifest)?;
    let plan = simplex_hir::build(&graph, request.shared).map_err(|error| error.to_string())?;
    let path = request.output.map_or_else(
        || destination(&plan.stem, platform, request.shared),
        Path::to_path_buf,
    );
    let folder = parent(&path);
    for (coordinate, mut dynamic) in plan.dynamics {
        let label = coordinate.stem();
        emit(
            platform,
            &label,
            &mut dynamic,
            (&folder.join(destination(&label, platform, true)), true),
        )?;
    }
    let mut main = plan.module;
    emit(platform, &plan.stem, &mut main, (&path, request.shared))
}

impl Loader<'_> {
    fn graph(&self, root: &Path, manifest: &Manifest) -> Result<Graph, String> {
        let mut packages = BTreeMap::new();
        let head = self.linked(root, manifest, &mut packages)?;
        Ok(Graph {
            root: head,
            packages,
        })
    }

    fn linked(
        &self,
        root: &Path,
        manifest: &Manifest,
        packages: &mut BTreeMap<Coordinate, Linked>,
    ) -> Result<Linked, String> {
        let mut dependencies = BTreeMap::new();
        for (alias, dependency) in &manifest.dependencies {
            let coordinate = self.resolve(alias, dependency, packages)?;
            dependencies.insert(alias.clone(), coordinate);
        }
        Ok(Linked {
            coordinate: manifest.library.clone(),
            root: root.to_path_buf(),
            entry: manifest.entry.clone(),
            dependencies,
        })
    }

    fn resolve(
        &self,
        alias: &str,
        dependency: &Dependency,
        packages: &mut BTreeMap<Coordinate, Linked>,
    ) -> Result<Coordinate, String> {
        match dependency {
            Dependency::Workspace => self.member(alias, packages),
            Dependency::Exact(coordinate) => self.external(coordinate, packages),
        }
    }

    fn member(
        &self,
        alias: &str,
        packages: &mut BTreeMap<Coordinate, Linked>,
    ) -> Result<Coordinate, String> {
        let Some(workspace) = self.workspace else {
            return Err(format!("missing workspace for dependency: {alias}"));
        };
        let found = workspace
            .members
            .get(alias)
            .or_else(|| {
                workspace
                    .members
                    .values()
                    .find(|(_, item)| item.library.name == alias)
            })
            .ok_or_else(|| format!("unknown workspace member: {alias}"))?;
        let coordinate = found.1.library.clone();
        if !packages.contains_key(&coordinate) {
            packages.insert(
                coordinate.clone(),
                placeholder(&coordinate, &found.0, &found.1),
            );
            let node = self.linked(&found.0, &found.1, packages)?;
            packages.insert(coordinate.clone(), node);
        }
        Ok(coordinate)
    }

    fn external(
        &self,
        coordinate: &Coordinate,
        packages: &mut BTreeMap<Coordinate, Linked>,
    ) -> Result<Coordinate, String> {
        if !packages.contains_key(coordinate) {
            let cached = self
                .store
                .fetch(coordinate)
                .map_err(|error| error.to_string())?;
            let manifest = Manifest::load(&cached.root).map_err(|error| error.to_string())?;
            if manifest.library != *coordinate {
                return Err("cached manifest coordinate mismatch".into());
            }
            packages.insert(
                coordinate.clone(),
                placeholder(coordinate, &cached.root, &manifest),
            );
            let node = self.linked(&cached.root, &manifest, packages)?;
            packages.insert(coordinate.clone(), node);
        }
        Ok(coordinate.clone())
    }
}

fn placeholder(coordinate: &Coordinate, root: &Path, manifest: &Manifest) -> Linked {
    Linked {
        coordinate: coordinate.clone(),
        root: root.to_path_buf(),
        entry: manifest.entry.clone(),
        dependencies: BTreeMap::new(),
    }
}

fn emit(
    platform: Platform,
    label: &str,
    module: &mut simplex_ir::Module,
    target: (&Path, bool),
) -> Result<(), String> {
    simplex_ir::prepare(module).map_err(|error| error.to_string())?;
    let artifact =
        simplex_target::compile(platform, label, module).map_err(|error| error.to_string())?;
    let bytes = simplex_object::emit(&artifact).map_err(|error| error.to_string())?;
    write(target.0, &bytes, target.1)
}

fn resolve(target: Option<&str>) -> Result<Platform, String> {
    match target {
        Some(name) => Platform::parse(name).ok_or_else(|| format!("unknown target: {name}")),
        None => Ok(Platform::host()),
    }
}

fn stem(entry: &Path) -> String {
    entry
        .file_stem()
        .and_then(|part| part.to_str())
        .unwrap_or("out")
        .to_owned()
}

pub fn destination(stem: &str, platform: Platform, shared: bool) -> PathBuf {
    match (platform, shared) {
        (Platform::Linux, true) => PathBuf::from(format!("lib{stem}.so")),
        (Platform::Windows, true) => PathBuf::from(format!("{stem}.dll")),
        (Platform::Apple, true) => PathBuf::from(format!("lib{stem}.dylib")),
        (Platform::Windows, false) => PathBuf::from(format!("{stem}.exe")),
        (Platform::Linux | Platform::Apple, false) => PathBuf::from(stem),
    }
}

pub fn write(path: &Path, bytes: &[u8], shared: bool) -> Result<(), String> {
    if let Some(folder) = path.parent()
        && !folder.as_os_str().is_empty()
    {
        fs::create_dir_all(folder).map_err(|error| format!("mkdir failed: {error}"))?;
    }
    let _ = fs::remove_file(path);
    fs::write(path, bytes).map_err(|error| format!("write failed: {error}"))?;
    #[cfg(unix)]
    if !shared {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(path, perms).map_err(|error| format!("chmod failed: {error}"))?;
    }
    Ok(())
}
