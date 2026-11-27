mod build;
mod release;
mod remote;

use clap::{Parser, Subcommand};
use remote::Context;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser)]
#[command(
    name = "simplex",
    version = version(),
    about = "Simplex (.sx) native compiler, package sealer and golden specimen verifier."
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
        #[command(flatten)]
        context: Context,
    },
    Seal {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        baseline: Option<String>,
        #[arg(long)]
        epoch: bool,
        #[command(flatten)]
        context: Context,
    },
    Publish {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        baseline: Option<String>,
        #[arg(long)]
        epoch: bool,
        #[command(flatten)]
        context: Context,
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
    let outcome = match cli.command {
        Some(Action::Check { cases }) => run(&cases),
        Some(Action::Format { file }) => format(&file),
        Some(Action::Build {
            entry,
            output,
            target,
            dylib,
            context,
        }) => build::compile(&build::Request {
            entry: &entry,
            output: output.as_deref(),
            target: target.as_deref(),
            shared: dylib,
            context: &context,
        }),
        Some(Action::Seal {
            path,
            output,
            baseline,
            epoch,
            context,
        }) => release::seal(&release::Options {
            path: &path,
            output: output.as_deref(),
            baseline: baseline.as_deref(),
            epoch,
            context: &context,
        }),
        Some(Action::Publish {
            path,
            baseline,
            epoch,
            context,
        }) => release::publish(&release::Options {
            path: &path,
            output: None,
            baseline: baseline.as_deref(),
            epoch,
            context: &context,
        }),
        None => {
            println!("{}", simplex::name());
            Ok(())
        }
    };
    if let Err(message) = outcome {
        eprintln!("{message}");
        process::exit(1);
    }
}

fn run(cases: &Path) -> Result<(), String> {
    match simplex::verify(cases) {
        Ok(report) => {
            println!(
                "verified {} domains ({} pass, {} reject)",
                report.domains, report.passed, report.rejected
            );
            Ok(())
        }
        Err(failure) => Err(failure.to_string()),
    }
}

fn format(file: &Path) -> Result<(), String> {
    let source =
        fs::read_to_string(file).map_err(|_| format!("unreadable file: {}", file.display()))?;
    let rendered = simplex::tokenize(&source)
        .and_then(|tokens| {
            simplex::surface::check(&tokens)?;
            let program = simplex::parser::parse(&tokens)?;
            simplex::control::check(&program)?;
            Ok(simplex::render(&program))
        })
        .map_err(|finding| finding.to_string())?;
    print!("{rendered}");
    Ok(())
}
