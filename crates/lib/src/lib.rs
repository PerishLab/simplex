pub mod lexer;
pub mod suite;

pub use lexer::{Code, Finding, Kind, Token, scan};
pub use suite::{Failure, Report, verify};

pub fn name() -> &'static str {
    "simplex"
}
