use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum Code {
    #[serde(rename = "lexical.comment")]
    Comment,
    #[serde(rename = "lexical.singleword")]
    Singleword,
    #[serde(rename = "control.if")]
    If,
    #[serde(rename = "lexical.literal")]
    Literal,
    #[serde(rename = "lexical.character")]
    Character,
}

impl Code {
    pub fn label(self) -> &'static str {
        match self {
            Self::Comment => "lexical.comment",
            Self::Singleword => "lexical.singleword",
            Self::If => "control.if",
            Self::Literal => "lexical.literal",
            Self::Character => "lexical.character",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Word,
    Number,
    Text,
    Symbol,
    Newline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    pub line: usize,
    pub column: usize,
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

pub fn scan(source: &str) -> Result<Vec<Token>, Finding> {
    Cursor::new(source).collect()
}

struct Cursor<'a> {
    source: &'a str,
    bytes: &'a [u8],
    offset: usize,
    line: usize,
    column: usize,
}

impl<'a> Cursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn next(&self) -> Option<u8> {
        self.bytes.get(self.offset + 1).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;
        if byte == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(byte)
    }

    fn fault(&self, code: Code, line: usize, column: usize) -> Finding {
        Finding { code, line, column }
    }

    fn collect(&mut self) -> Result<Vec<Token>, Finding> {
        let mut tokens = Vec::new();
        while let Some(byte) = self.peek() {
            match byte {
                b' ' | b'\t' => {
                    self.bump();
                }
                b'\r' => {
                    return Err(self.fault(Code::Character, self.line, self.column));
                }
                b'\n' => {
                    let (line, column) = (self.line, self.column);
                    self.bump();
                    tokens.push(Token {
                        kind: Kind::Newline,
                        text: "\n".into(),
                        line,
                        column,
                    });
                }
                b'/' if matches!(self.next(), Some(b'/' | b'*')) => {
                    return Err(self.fault(Code::Comment, self.line, self.column));
                }
                _ => tokens.push(self.step()?),
            }
        }
        Ok(tokens)
    }

    fn step(&mut self) -> Result<Token, Finding> {
        let byte = self.peek().unwrap_or_default();
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.word(),
            b'0'..=b'9' => self.number(),
            b'"' => self.literal(),
            _ => self.symbol(),
        }
    }

    fn word(&mut self) -> Result<Token, Finding> {
        let (start, line, column) = (self.offset, self.line, self.column);
        while let Some(byte) = self.peek() {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                self.bump();
            } else {
                break;
            }
        }
        let text = &self.source[start..self.offset];
        if text == "if" {
            return Err(self.fault(Code::If, line, column));
        }
        if !single(text) {
            return Err(self.fault(Code::Singleword, line, column));
        }
        Ok(Token {
            kind: Kind::Word,
            text: text.into(),
            line,
            column,
        })
    }

    fn number(&mut self) -> Result<Token, Finding> {
        let (start, line, column) = (self.offset, self.line, self.column);
        while let Some(byte) = self.peek() {
            if byte.is_ascii_digit() {
                self.bump();
            } else if byte.is_ascii_alphabetic() || byte == b'_' {
                return Err(self.fault(Code::Literal, line, column));
            } else {
                break;
            }
        }
        Ok(Token {
            kind: Kind::Number,
            text: self.source[start..self.offset].into(),
            line,
            column,
        })
    }

    fn literal(&mut self) -> Result<Token, Finding> {
        let (start, line, column) = (self.offset, self.line, self.column);
        self.bump();
        while let Some(byte) = self.peek() {
            if matches!(byte, b'\n' | b'\r') {
                return Err(self.fault(Code::Literal, line, column));
            }
            self.bump();
            if byte == b'\\' && self.bump().is_none() {
                return Err(self.fault(Code::Literal, line, column));
            }
            if byte == b'"' {
                return Ok(Token {
                    kind: Kind::Text,
                    text: self.source[start..self.offset].into(),
                    line,
                    column,
                });
            }
        }
        Err(self.fault(Code::Literal, line, column))
    }

    fn symbol(&mut self) -> Result<Token, Finding> {
        let (start, line, column) = (self.offset, self.line, self.column);
        let first = self.peek().unwrap_or_default();
        let second = self.next().unwrap_or_default();
        let pair = matches!(
            (first, second),
            (b':', b'=')
                | (b'=', b'>')
                | (b'-', b'>')
                | (b'=', b'=')
                | (b'!', b'=')
                | (b'<', b'=')
                | (b'>', b'=')
                | (b'.', b'.')
                | (b':', b':')
        );
        if pair {
            self.bump();
            self.bump();
        } else if punctuation(first) {
            self.bump();
        } else {
            return Err(self.fault(Code::Character, line, column));
        }
        Ok(Token {
            kind: Kind::Symbol,
            text: self.source[start..self.offset].into(),
            line,
            column,
        })
    }
}

fn single(text: &str) -> bool {
    if text == "_" {
        return true;
    }
    if text.contains('_') {
        return false;
    }
    let mut lowered = false;
    for (index, ch) in text.chars().enumerate() {
        if ch.is_ascii_lowercase() {
            lowered = true;
        } else if ch.is_ascii_uppercase() && (index > 0 || lowered) {
            return false;
        }
    }
    true
}

fn punctuation(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b','
            | b'.'
            | b':'
            | b'='
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'&'
            | b'|'
            | b'!'
            | b'<'
            | b'>'
            | b'?'
    )
}
