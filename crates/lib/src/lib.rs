pub mod code;
pub mod control;
pub mod format;
pub mod lexer;
pub mod model;
pub mod module;
pub mod parser;
pub mod suite;
pub mod tree;

pub use code::{Code, Finding, Kind, Token};
pub use format::render;
pub use lexer::{surface, tokenize};
pub use suite::{Failure, Report, verify};
pub use tree::Program;

use std::path::Path;

pub fn check(source: &str) -> Result<Program, Finding> {
    inspect(None, None, source)
}

pub fn inspect(file: Option<&str>, dir: Option<&Path>, source: &str) -> Result<Program, Finding> {
    if let Some(name) = file
        && matches!(name, "mod.sx" | "index.sx" | "lib.sx")
    {
        return Err(Finding {
            code: Code::Barrel,
            line: 1,
            column: 1,
        });
    }
    let tokens = tokenize(source)?;
    surface::check(&tokens)?;
    let program = parser::parse(&tokens)?;
    control::check(&program)?;
    module::check(file, dir, &program)?;
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
    inspect(None, None, source)?;
    Ok(tokens)
}

pub fn name() -> &'static str {
    "simplex"
}
