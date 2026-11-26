use crate::code::{Code, Finding, Kind, Token};

pub fn check(tokens: &[Token]) -> Result<(), Finding> {
    let surface = Surface { tokens };
    surface.casts()?;
    surface.angles()?;
    surface.lines()
}

struct Surface<'a> {
    tokens: &'a [Token],
}

impl<'a> Surface<'a> {
    fn get(&self, index: usize) -> Option<&'a Token> {
        self.tokens.get(index)
    }

    fn casts(&self) -> Result<(), Finding> {
        for (index, token) in self.tokens.iter().enumerate() {
            if token.kind == Kind::Word
                && self
                    .get(index + 1)
                    .is_some_and(|next| next.symbol("!") && !next.spaced)
            {
                return Err(token.fault(Code::Macro));
            }
            if token.kind == Kind::Word && token.text == "as" && !self.dotted(index) {
                return Err(token.fault(Code::Cast));
            }
            if token.kind == Kind::Symbol && token.text == "::" && self.symbol(index + 1, "<") {
                return Err(token.fault(Code::Cast));
            }
            if token.kind == Kind::Symbol && token.text == "::" {
                return Err(token.fault(Code::Import));
            }
            if self.symbol(index, ".") && self.converter(index + 1) && self.literal(index) {
                return Err(token.fault(Code::Cast));
            }
        }
        Ok(())
    }

    fn dotted(&self, index: usize) -> bool {
        index > 0 && self.symbol(index - 1, ".")
    }

    fn symbol(&self, index: usize, text: &str) -> bool {
        self.get(index)
            .is_some_and(|token| token.kind == Kind::Symbol && token.text == text)
    }

    fn converter(&self, index: usize) -> bool {
        self.get(index).is_some_and(|token| {
            token.kind == Kind::Word && matches!(token.text.as_str(), "into" | "as")
        })
    }

    fn literal(&self, dot: usize) -> bool {
        if dot == 0 {
            return false;
        }
        let prev = &self.tokens[dot - 1];
        if prev.atom() {
            return true;
        }
        prev.kind == Kind::Symbol && prev.text == ")" && self.group(dot - 1)
    }

    fn group(&self, close: usize) -> bool {
        let mut depth = 0usize;
        let mut index = close;
        loop {
            depth = step(&self.tokens[index], depth);
            if depth == 0 {
                return self.constant(index + 1, close);
            }
            if index == 0 {
                return false;
            }
            index -= 1;
        }
    }

    fn constant(&self, start: usize, end: usize) -> bool {
        let body: Vec<&Token> = self.tokens[start..end]
            .iter()
            .filter(|token| token.kind != Kind::Newline)
            .collect();
        match body.as_slice() {
            [one] => one.atom(),
            [sign, one] => sign.kind == Kind::Symbol && sign.text == "-" && one.atom(),
            [open, .., close] if open.text == "(" && close.text == ")" => {
                self.constant(start + 1, end - 1)
            }
            _ => false,
        }
    }

    fn angles(&self) -> Result<(), Finding> {
        let mut generics = 0usize;
        for (index, token) in self.tokens.iter().enumerate() {
            if token.kind == Kind::Newline && generics > 0 {
                return Err(token.fault(Code::Spacing));
            }
            if self.symbol(index, "<") {
                generics = self.open(index, token, generics)?;
            } else if self.symbol(index, ">") {
                generics = self.close(index, token, generics)?;
            }
        }
        if let Some(last) = self.tokens.last().filter(|_| generics > 0) {
            return Err(last.fault(Code::Spacing));
        }
        Ok(())
    }

    fn open(&self, index: usize, token: &Token, generics: usize) -> Result<usize, Finding> {
        let attached = index > 0 && self.tokens[index - 1].kind == Kind::Word && !token.spaced;
        let bound = index > 0 && self.binder(index - 1) && token.spaced;
        let next = self.get(index + 1);
        if attached || bound {
            if next.is_none_or(|item| item.spaced || item.kind == Kind::Newline) {
                return Err(token.fault(Code::Spacing));
            }
            return Ok(generics + 1);
        }
        if generics > 0 || !token.spaced || next.is_none_or(|item| !item.spaced) {
            return Err(token.fault(Code::Spacing));
        }
        Ok(0)
    }

    fn binder(&self, index: usize) -> bool {
        self.symbol(index, ":=") || self.symbol(index, "=") || self.tokens[index].word("async")
    }

    fn close(&self, index: usize, token: &Token, generics: usize) -> Result<usize, Finding> {
        let next = self.get(index + 1);
        if generics > 0 {
            if token.spaced || next.is_some_and(Token::glued) {
                return Err(token.fault(Code::Spacing));
            }
            return Ok(generics - 1);
        }
        if !token.spaced || next.is_none_or(|item| !item.spaced) {
            return Err(token.fault(Code::Spacing));
        }
        Ok(0)
    }

    fn lines(&self) -> Result<(), Finding> {
        let mut rounds = 0usize;
        let mut head = true;
        let mut pending: Option<&Token> = None;
        for (index, token) in self.tokens.iter().enumerate() {
            if token.kind == Kind::Newline {
                head = true;
                continue;
            }
            let fresh = head && rounds == 0 && pending.is_none();
            if fresh && self.leading(index, token) {
                return Err(token.fault(Code::Continuation));
            }
            rounds = bracket(rounds, token)?;
            head = false;
            pending = trailing(token);
        }
        if let Some(token) = pending {
            return Err(token.fault(Code::Continuation));
        }
        Ok(())
    }

    fn leading(&self, index: usize, token: &Token) -> bool {
        if token.kind != Kind::Symbol {
            return false;
        }
        if matches!(token.text.as_str(), "-" | "&") {
            return self.get(index + 1).is_some_and(|next| next.spaced);
        }
        continuation(&token.text)
    }
}

fn step(token: &Token, depth: usize) -> usize {
    if token.kind != Kind::Symbol {
        return depth;
    }
    match token.text.as_str() {
        ")" => depth + 1,
        "(" => depth.saturating_sub(1),
        _ => depth,
    }
}

fn bracket(rounds: usize, token: &Token) -> Result<usize, Finding> {
    if token.kind != Kind::Symbol {
        return Ok(rounds);
    }
    match token.text.as_str() {
        "(" | "[" => Ok(rounds + 1),
        ")" | "]" if rounds > 0 => Ok(rounds - 1),
        ")" | "]" => Err(token.fault(Code::Character)),
        _ => Ok(rounds),
    }
}

fn trailing(token: &Token) -> Option<&Token> {
    if token.kind == Kind::Symbol
        && (continuation(&token.text) || matches!(token.text.as_str(), "-" | "=" | ":="))
    {
        Some(token)
    } else {
        None
    }
}

fn continuation(text: &str) -> bool {
    matches!(
        text,
        "+" | "*"
            | "/"
            | "%"
            | "."
            | "=="
            | "!="
            | "<"
            | "<="
            | ">"
            | ">="
            | "&&"
            | "||"
            | "&"
            | "|"
            | "^"
            | "<<"
            | ">>"
    )
}
