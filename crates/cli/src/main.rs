use clap::{Parser, Subcommand};
use simplex_target::Platform;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser)]
#[command(
    name = "simplex",
    version = version(),
    about = "Simplex (.sx) native compiler and golden specimen verifier."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    Check {
        #[arg(default_value = "cases")]
        cases: PathBuf,
    },
    Format {
        file: PathBuf,
    },
    Build {
        entry: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        dylib: bool,
    },
}

fn version() -> &'static str {
    plumb::version!("SIMPLEX")
}

fn main() {
    if let Err(error) = plumb::identity!("SIMPLEX") {
        eprintln!("{error}");
        process::exit(1);
    }
    let cli = Cli::parse();
    match cli.command {
        Some(Action::Check { cases }) => run(&cases),
        Some(Action::Format { file }) => format(&file),
        Some(Action::Build {
            entry,
            output,
            target,
            dylib,
        }) => build(&entry, output.as_deref(), target.as_deref(), dylib),
        None => println!("{}", simplex::name()),
    }
}

fn run(cases: &Path) {
    match simplex::verify(cases) {
        Ok(report) => println!(
            "verified {} domains ({} pass, {} reject)",
            report.domains, report.passed, report.rejected
        ),
        Err(failure) => {
            eprintln!("{failure}");
            process::exit(1);
        }
    }
}

fn format(file: &Path) {
    let Ok(source) = fs::read_to_string(file) else {
        eprintln!("unreadable file: {}", file.display());
        process::exit(1);
    };
    match simplex::tokenize(&source).and_then(|tokens| {
        simplex::surface::check(&tokens)?;
        let program = simplex::parser::parse(&tokens)?;
        simplex::control::check(&program)?;
        Ok(simplex::render(&program))
    }) {
        Ok(rendered) => print!("{rendered}"),
        Err(finding) => {
            eprintln!("{finding}");
            process::exit(1);
        }
    }
}

fn build(entry: &Path, output: Option<&Path>, target: Option<&str>, shared: bool) {
    if let Err(message) = compile(entry, output, target, shared) {
        eprintln!("{message}");
        process::exit(1);
    }
}

fn compile(
    entry: &Path,
    output: Option<&Path>,
    target: Option<&str>,
    shared: bool,
) -> Result<(), String> {
    let platform = resolve(target)?;
    let stem = stem(entry);
    let path = output.map_or_else(|| destination(&stem, platform, shared), Path::to_path_buf);
    let mut module =
        simplex_hir::translate("app", entry, shared).map_err(|error| error.to_string())?;
    simplex_ir::prepare(&mut module).map_err(|error| error.to_string())?;
    let artifact =
        simplex_target::compile(platform, &stem, &module).map_err(|error| error.to_string())?;
    let bytes = simplex_object::emit(&artifact).map_err(|error| error.to_string())?;
    write(&path, &bytes, shared)
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

fn destination(stem: &str, platform: Platform, shared: bool) -> PathBuf {
    match (platform, shared) {
        (Platform::Linux, true) => PathBuf::from(format!("lib{stem}.so")),
        (Platform::Windows, true) => PathBuf::from(format!("{stem}.dll")),
        (Platform::Apple, true) => PathBuf::from(format!("lib{stem}.dylib")),
        (Platform::Windows, false) => PathBuf::from(format!("{stem}.exe")),
        (Platform::Linux | Platform::Apple, false) => PathBuf::from(stem),
    }
}

fn write(path: &Path, bytes: &[u8], shared: bool) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| format!("mkdir failed: {error}"))?;
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
