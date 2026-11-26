pub mod code;
pub mod control;
pub mod format;
pub mod lexer;
pub mod model;
pub mod parser;
pub mod suite;
pub mod tree;

pub use code::{Code, Finding, Kind, Token};
pub use format::render;
pub use lexer::{surface, tokenize};
pub use suite::{Failure, Report, verify};
pub use tree::Program;

pub fn check(source: &str) -> Result<Program, Finding> {
    let tokens = tokenize(source)?;
    surface::check(&tokens)?;
    let program = parser::parse(&tokens)?;
    control::check(&program)?;
    model::check(&program)?;
    let formatted = format::verify(source, &program)?;
    let reparsed = parser::parse(&tokenize(&formatted)?)?;
    if reparsed != program {
        return Err(Finding {
            code: Code::Format,
            line: 1,
            column: 1,
        });
    }
    Ok(program)
}

pub fn scan(source: &str) -> Result<Vec<Token>, Finding> {
    let tokens = tokenize(source)?;
    surface::check(&tokens)?;
    let program = parser::parse(&tokens)?;
    control::check(&program)?;
    model::check(&program)?;
    format::verify(source, &program)?;
    Ok(tokens)
}

pub fn name() -> &'static str {
    "simplex"
}
