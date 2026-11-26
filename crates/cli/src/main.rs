use clap::{Parser, Subcommand};
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
            dylib,
        }) => build(&entry, output.as_deref(), dylib),
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

fn build(entry: &Path, output: Option<&Path>, shared: bool) {
    let _ = simplex_runtime::STACK;
    let target = output.map_or_else(|| destination(entry, shared), Path::to_path_buf);
    let result = simplex_hir::lower("app", entry, shared)
        .map_err(|e| e.to_string())
        .and_then(|unit| simplex_emit::compile(&unit).map_err(|e| e.to_string()))
        .and_then(|plan| simplex_elf::write(&plan).map_err(|e| e.to_string()))
        .and_then(|bytes| write(&target, &bytes, shared));
    if let Err(message) = result {
        eprintln!("{message}");
        process::exit(1);
    }
}

fn destination(entry: &Path, shared: bool) -> PathBuf {
    let stem = entry.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    if shared {
        PathBuf::from(format!("lib{stem}.so"))
    } else {
        PathBuf::from(stem)
    }
}

fn write(path: &Path, bytes: &[u8], shared: bool) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir failed: {e}"))?;
    }
    let _ = fs::remove_file(path);
    fs::write(path, bytes).map_err(|e| format!("write failed: {e}"))?;
    #[cfg(unix)]
    if !shared {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(path, perms).map_err(|e| format!("chmod failed: {e}"))?;
    }
    Ok(())
}
