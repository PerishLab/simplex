use crate::code::{Code, Finding};

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
}
