pub mod classify;
pub mod code;
pub mod lexer;
pub mod suite;
pub mod surface;
pub mod token;

pub use code::{Code, Finding};
pub use lexer::tokenize;
pub use suite::{Failure, Report, verify};
pub use token::{Kind, Token};

pub fn scan(source: &str) -> Result<Vec<Token>, Finding> {
    let tokens = tokenize(source)?;
    surface::check(&tokens)?;
    Ok(tokens)
}

pub fn name() -> &'static str {
    "simplex"
}
