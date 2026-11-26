use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser)]
#[command(
    name = "simplex",
    version = version(),
    about = "Simplex (.sx) prototype frontend and golden specimen verifier."
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
