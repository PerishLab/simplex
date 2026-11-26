use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum Code {
    #[serde(rename = "lexical.comment")]
    Comment,
    #[serde(rename = "lexical.character")]
    Character,
    #[serde(rename = "lexical.semicolon")]
    Semicolon,
    #[serde(rename = "lexical.singleword")]
    Singleword,
    #[serde(rename = "lexical.literal")]
    Literal,
    #[serde(rename = "lexical.declaration")]
    Declaration,
    #[serde(rename = "lexical.assignment")]
    Assignment,
    #[serde(rename = "lexical.cast")]
    Cast,
    #[serde(rename = "lexical.spacing")]
    Spacing,
    #[serde(rename = "lexical.continuation")]
    Continuation,
    #[serde(rename = "control.if")]
    If,
}

impl Code {
    pub fn label(self) -> &'static str {
        match self {
            Self::Comment => "lexical.comment",
            Self::Character => "lexical.character",
            Self::Semicolon => "lexical.semicolon",
            Self::Singleword => "lexical.singleword",
            Self::Literal => "lexical.literal",
            Self::Declaration => "lexical.declaration",
            Self::Assignment => "lexical.assignment",
            Self::Cast => "lexical.cast",
            Self::Spacing => "lexical.spacing",
            Self::Continuation => "lexical.continuation",
            Self::If => "control.if",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: Code,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for Finding {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}:{}: {}", self.line, self.column, self.code)
    }
}
