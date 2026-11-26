use crate::classify::{digit, single, span};
use crate::code::{Code, Finding};
use crate::token::{Kind, Token};

#[derive(Clone, Copy)]
struct Anchor {
    start: usize,
    line: usize,
    column: usize,
    spaced: bool,
}

impl Anchor {
    fn fault(self, code: Code) -> Finding {
        Finding {
            code,
            line: self.line,
            column: self.column,
        }
    }
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, Finding> {
    Cursor::new(source).collect()
}

struct Cursor<'a> {
    source: &'a str,
    bytes: &'a [u8],
    offset: usize,
    line: usize,
    column: usize,
    gap: bool,
}

impl<'a> Cursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            offset: 0,
            line: 1,
            column: 1,
            gap: true,
        }
    }

    fn mark(&self) -> Anchor {
        Anchor {
            start: self.offset,
            line: self.line,
            column: self.column,
            spaced: self.gap,
        }
    }

    fn emit(&self, mark: Anchor, kind: Kind) -> Token {
        Token {
            kind,
            text: self.source[mark.start..self.offset].into(),
            line: mark.line,
            column: mark.column,
            spaced: mark.spaced,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn next(&self) -> Option<u8> {
        self.bytes.get(self.offset + 1).copied()
    }

    fn third(&self) -> Option<u8> {
        self.bytes.get(self.offset + 2).copied()
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

    fn collect(&mut self) -> Result<Vec<Token>, Finding> {
        let mut tokens = Vec::new();
        while let Some(byte) = self.peek() {
            match byte {
                b' ' | b'\t' => {
                    self.bump();
                    self.gap = true;
                }
                b'\r' => return Err(self.mark().fault(Code::Character)),
                b';' => return Err(self.mark().fault(Code::Semicolon)),
                b'\n' => {
                    let mark = self.mark();
                    self.bump();
                    self.gap = true;
                    tokens.push(self.emit(mark, Kind::Newline));
                }
                b'/' if matches!(self.next(), Some(b'/' | b'*')) => {
                    return Err(self.mark().fault(Code::Comment));
                }
                _ => {
                    let token = self.step()?;
                    self.gap = false;
                    tokens.push(token);
                }
            }
        }
        Ok(tokens)
    }

    fn step(&mut self) -> Result<Token, Finding> {
        match self.peek().unwrap_or_default() {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.word(),
            b'0'..=b'9' => self.number(),
            b'"' => self.text(),
            b'\'' => self.byte(),
            _ => self.symbol(),
        }
    }

    fn word(&mut self) -> Result<Token, Finding> {
        let mark = self.mark();
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.bump();
        }
        let slice = &self.source[mark.start..self.offset];
        match slice {
            "if" => Err(mark.fault(Code::If)),
            "let" | "mut" => Err(mark.fault(Code::Declaration)),
            _ if !single(slice) => Err(mark.fault(Code::Singleword)),
            _ => Ok(self.emit(mark, Kind::Word)),
        }
    }

    fn number(&mut self) -> Result<Token, Finding> {
        let mark = self.mark();
        if self.peek() == Some(b'0') {
            match self.next() {
                Some(b'x') => return self.prefixed(mark, 16),
                Some(b'b') => return self.prefixed(mark, 2),
                Some(b'o') => return self.prefixed(mark, 8),
                Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                    return Err(mark.fault(Code::Literal));
                }
                _ => {}
            }
        }
        self.decimal(mark)?;
        Ok(self.emit(mark, Kind::Number))
    }

    fn prefixed(&mut self, mark: Anchor, radix: u32) -> Result<Token, Finding> {
        self.bump();
        self.bump();
        let mut count = 0usize;
        while let Some(byte) = self.peek() {
            if digit(byte, radix) {
                self.bump();
                count += 1;
            } else if byte.is_ascii_alphanumeric() || byte == b'_' {
                return Err(mark.fault(Code::Literal));
            } else {
                break;
            }
        }
        if count == 0 || (self.peek() == Some(b'.') && self.next() != Some(b'.')) {
            return Err(mark.fault(Code::Literal));
        }
        Ok(self.emit(mark, Kind::Number))
    }

    fn decimal(&mut self, mark: Anchor) -> Result<(), Finding> {
        self.digits(mark)?;
        if self.peek() == Some(b'.') && self.next() != Some(b'.') {
            self.bump();
            if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return Err(mark.fault(Code::Literal));
            }
            self.digits(mark)?;
            if self.peek() == Some(b'.') && self.next() != Some(b'.') {
                return Err(mark.fault(Code::Literal));
            }
        }
        Ok(())
    }

    fn digits(&mut self, mark: Anchor) -> Result<(), Finding> {
        while let Some(byte) = self.peek() {
            if byte.is_ascii_digit() {
                self.bump();
            } else if byte.is_ascii_alphabetic() || byte == b'_' {
                return Err(mark.fault(Code::Literal));
            } else {
                break;
            }
        }
        Ok(())
    }

    fn text(&mut self) -> Result<Token, Finding> {
        let mark = self.mark();
        self.bump();
        while let Some(byte) = self.peek() {
            match byte {
                b'\n' | b'\r' => return Err(mark.fault(Code::Literal)),
                b'\\' => {
                    self.bump();
                    self.escape(mark)?;
                }
                b'"' => {
                    self.bump();
                    return Ok(self.emit(mark, Kind::Text));
                }
                _ => {
                    self.bump();
                }
            }
        }
        Err(mark.fault(Code::Literal))
    }

    fn byte(&mut self) -> Result<Token, Finding> {
        let mark = self.mark();
        self.bump();
        match self.peek() {
            None | Some(b'\'' | b'\n' | b'\r') => return Err(mark.fault(Code::Literal)),
            Some(b'\\') => {
                self.bump();
                self.escape(mark)?;
            }
            Some(_) => {
                self.bump();
            }
        }
        if self.peek() != Some(b'\'') {
            return Err(mark.fault(Code::Literal));
        }
        self.bump();
        Ok(self.emit(mark, Kind::Byte))
    }

    fn escape(&mut self, mark: Anchor) -> Result<(), Finding> {
        match self.bump() {
            Some(b'n' | b'r' | b't' | b'0' | b'\\' | b'"' | b'\'') => Ok(()),
            _ => Err(mark.fault(Code::Literal)),
        }
    }

    fn symbol(&mut self) -> Result<Token, Finding> {
        let mark = self.mark();
        let (first, second, third) = (
            self.peek().unwrap_or_default(),
            self.next().unwrap_or_default(),
            self.third().unwrap_or_default(),
        );
        if matches!((first, second), (b'+', b'+') | (b'-', b'-')) {
            return Err(mark.fault(Code::Assignment));
        }
        if first == b'.' && second.is_ascii_digit() {
            return Err(mark.fault(Code::Literal));
        }
        let width = span(first, second, third, mark.spaced);
        if width == 0 {
            return Err(mark.fault(Code::Character));
        }
        for _ in 0..width {
            self.bump();
        }
        Ok(self.emit(mark, Kind::Symbol))
    }
}
