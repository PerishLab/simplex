use clap::{Parser, Subcommand};
use std::path::PathBuf;
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
        Some(Action::Check { cases }) => match simplex::verify(&cases) {
            Ok(report) => {
                println!(
                    "verified {} domains ({} pass, {} reject)",
                    report.domains, report.passed, report.rejected
                );
            }
            Err(failure) => {
                eprintln!("{failure}");
                process::exit(1);
            }
        },
        None => println!("{}", simplex::name()),
    }
}
