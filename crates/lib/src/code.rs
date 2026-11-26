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
    #[serde(rename = "control.while")]
    While,
    #[serde(rename = "control.return")]
    Return,
    #[serde(rename = "control.exception")]
    Exception,
    #[serde(rename = "control.ternary")]
    Ternary,
    #[serde(rename = "control.declaration")]
    Signature,
    #[serde(rename = "control.inline")]
    Inline,
    #[serde(rename = "control.switch")]
    Switch,
    #[serde(rename = "control.bucket")]
    Bucket,
    #[serde(rename = "control.arm")]
    Arm,
    #[serde(rename = "control.binding")]
    Binding,
    #[serde(rename = "control.depth")]
    Depth,
    #[serde(rename = "control.syntax")]
    Syntax,
    #[serde(rename = "control.format")]
    Format,
    #[serde(rename = "model.discard")]
    Discard,
    #[serde(rename = "model.inheritance")]
    Inheritance,
    #[serde(rename = "model.tuple")]
    Tuple,
    #[serde(rename = "model.declaration")]
    Model,
    #[serde(rename = "model.conformance")]
    Conformance,
    #[serde(rename = "model.bucket")]
    Outcome,
    #[serde(rename = "model.switch")]
    Scrutinee,
    #[serde(rename = "model.coercion")]
    Coercion,
    #[serde(rename = "model.binding")]
    Usage,
    #[serde(rename = "module.barrel")]
    Barrel,
    #[serde(rename = "module.reexport")]
    Reexport,
    #[serde(rename = "module.import")]
    Import,
    #[serde(rename = "module.cycle")]
    Cycle,
    #[serde(rename = "module.visibility")]
    Visibility,
    #[serde(rename = "module.extern")]
    Extern,
    #[serde(rename = "module.arity")]
    Arity,
    #[serde(rename = "module.path")]
    Path,
    #[serde(rename = "borrow.moved")]
    Moved,
    #[serde(rename = "borrow.mutable")]
    Mutable,
    #[serde(rename = "borrow.conflict")]
    Conflict,
    #[serde(rename = "borrow.escape")]
    Escape,
    #[serde(rename = "borrow.storage")]
    Storage,
    #[serde(rename = "borrow.lifetime")]
    Lifetime,
    #[serde(rename = "borrow.copy")]
    Replica,
    #[serde(rename = "async.context")]
    Context,
    #[serde(rename = "async.detached")]
    Detached,
    #[serde(rename = "async.cancel")]
    Cancel,
    #[serde(rename = "async.borrow")]
    Concurrent,
    #[serde(rename = "async.conformance")]
    Mismatch,
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
            Self::While => "control.while",
            Self::Return => "control.return",
            Self::Exception => "control.exception",
            Self::Ternary => "control.ternary",
            Self::Signature => "control.declaration",
            Self::Inline => "control.inline",
            Self::Switch => "control.switch",
            Self::Bucket => "control.bucket",
            Self::Arm => "control.arm",
            Self::Binding => "control.binding",
            Self::Depth => "control.depth",
            Self::Syntax => "control.syntax",
            Self::Format => "control.format",
            Self::Discard => "model.discard",
            Self::Inheritance => "model.inheritance",
            Self::Tuple => "model.tuple",
            Self::Model => "model.declaration",
            Self::Conformance => "model.conformance",
            Self::Outcome => "model.bucket",
            Self::Scrutinee => "model.switch",
            Self::Coercion => "model.coercion",
            Self::Usage => "model.binding",
            Self::Barrel => "module.barrel",
            Self::Reexport => "module.reexport",
            Self::Import => "module.import",
            Self::Cycle => "module.cycle",
            Self::Visibility => "module.visibility",
            Self::Extern => "module.extern",
            Self::Arity => "module.arity",
            Self::Path => "module.path",
            Self::Moved => "borrow.moved",
            Self::Mutable => "borrow.mutable",
            Self::Conflict => "borrow.conflict",
            Self::Escape => "borrow.escape",
            Self::Storage => "borrow.storage",
            Self::Lifetime => "borrow.lifetime",
            Self::Replica => "borrow.copy",
            Self::Context => "async.context",
            Self::Detached => "async.detached",
            Self::Cancel => "async.cancel",
            Self::Concurrent => "async.borrow",
            Self::Mismatch => "async.conformance",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Word,
    Number,
    Text,
    Byte,
    Symbol,
    Newline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    pub line: usize,
    pub column: usize,
    pub spaced: bool,
}

impl Token {
    pub fn fault(&self, code: Code) -> Finding {
        Finding {
            code,
            line: self.line,
            column: self.column,
        }
    }

    pub fn atom(&self) -> bool {
        matches!(self.kind, Kind::Number | Kind::Text | Kind::Byte)
            || (self.kind == Kind::Word && matches!(self.text.as_str(), "true" | "false"))
    }

    pub fn glued(&self) -> bool {
        !self.spaced && !matches!(self.kind, Kind::Symbol | Kind::Newline)
    }

    pub fn word(&self, expected: &str) -> bool {
        self.kind == Kind::Word && self.text == expected
    }

    pub fn symbol(&self, expected: &str) -> bool {
        self.kind == Kind::Symbol && self.text == expected
    }
}
