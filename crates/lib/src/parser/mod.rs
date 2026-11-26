mod control;
mod decl;
mod expr;
mod imports;
mod postfix;

use crate::code::{Code, Finding, Kind, Token};
use crate::tree::{Binding, Expression, Item, Program, Statement};

pub fn parse(tokens: &[Token]) -> Result<Program, Finding> {
    let mut stream = Stream { tokens, index: 0 };
    let items = stream.sequence(false)?;
    if let Some(extra) = stream.peek() {
        return Err(extra.fault(Code::Syntax));
    }
    Ok(Program { items })
}

pub(crate) struct Stream<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) index: usize,
}

impl<'a> Stream<'a> {
    pub(crate) fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.index)
    }

    pub(crate) fn ahead(&self, offset: usize) -> Option<&'a Token> {
        self.tokens.get(self.index + offset)
    }

    pub(crate) fn bump(&mut self) -> Option<&'a Token> {
        let token = self.tokens.get(self.index)?;
        self.index += 1;
        Some(token)
    }

    pub(crate) fn fault(&self, code: Code) -> Finding {
        match self.peek().or_else(|| self.tokens.last()) {
            Some(token) => token.fault(code),
            None => Finding {
                code,
                line: 1,
                column: 1,
            },
        }
    }

    pub(crate) fn newlines(&mut self) -> usize {
        let mut count = 0usize;
        while self.peek().is_some_and(|token| token.kind == Kind::Newline) {
            self.bump();
            count += 1;
        }
        count
    }

    pub(crate) fn word(&mut self) -> Result<&'a Token, Finding> {
        match self.peek() {
            Some(token) if token.kind == Kind::Word => Ok(self.bump().unwrap()),
            _ => Err(self.fault(Code::Syntax)),
        }
    }

    pub(crate) fn expect(&mut self, symbol: &str) -> Result<&'a Token, Finding> {
        match self.peek() {
            Some(token) if token.symbol(symbol) => Ok(self.bump().unwrap()),
            _ => Err(self.fault(Code::Syntax)),
        }
    }

    pub(crate) fn sequence(&mut self, braced: bool) -> Result<Vec<Item>, Finding> {
        let mut items = Vec::new();
        let initial = self.newlines();
        if (braced && initial > 1) || (!braced && initial > 0) {
            return Err(self.fault(Code::Format));
        }
        let mut blank = false;
        while let Some(token) = self.peek() {
            if braced && token.symbol("}") {
                break;
            }
            let (line, column) = (token.line, token.column);
            let statement = self.statement(braced)?;
            items.push(Item {
                statement,
                blank,
                line,
                column,
            });
            let breaks = self.newlines();
            if breaks > 2 {
                return Err(self.fault(Code::Format));
            }
            blank = breaks == 2;
            if breaks == 0 && self.peek().is_some_and(|next| !braced || !next.symbol("}")) {
                return Err(self.fault(Code::Syntax));
            }
        }
        if blank {
            return Err(self.fault(Code::Format));
        }
        Ok(items)
    }

    fn statement(&mut self, braced: bool) -> Result<Statement, Finding> {
        let Some(token) = self.peek() else {
            return Err(self.fault(Code::Syntax));
        };
        if token.word("pub") {
            return self.exposed(braced);
        }
        if let Some(code) = banned(token, self.ahead(1)) {
            return Err(token.fault(code));
        }
        if token.word("var") {
            return self.variable();
        }
        if token.word("comptime") && self.ahead(1).is_some_and(|next| next.word("for")) {
            self.bump();
            return self.iteration(true);
        }
        if token.word("for") {
            return self.iteration(false);
        }
        if token.word("loop") {
            let start = self.bump().unwrap();
            return Ok(Statement::Loop(self.repetition(start)?));
        }
        if token.kind == Kind::Word
            && capitalized(&token.text)
            && let Some(model) = self.nominal((false, false))?
        {
            return Ok(model);
        }
        if token.kind == Kind::Word && self.declaration() {
            return self.declared((false, false));
        }
        self.action()
    }

    fn exposed(&mut self, braced: bool) -> Result<Statement, Finding> {
        let kw = self.bump().unwrap();
        if braced || self.peek().is_some_and(|tok| tok.symbol("(")) {
            return Err(kw.fault(Code::Visibility));
        }
        let dynamic = if self.peek().is_some_and(|tok| tok.word("dylib")) {
            self.bump();
            true
        } else {
            false
        };
        let Some(next) = self.peek() else {
            return Err(kw.fault(Code::Visibility));
        };
        if next.kind == Kind::Word
            && capitalized(&next.text)
            && let Some(model) = self.nominal((true, dynamic))?
        {
            return Ok(model);
        }
        if next.kind == Kind::Word && self.declaration() {
            return self.declared((true, dynamic));
        }
        Err(kw.fault(Code::Visibility))
    }

    fn action(&mut self) -> Result<Statement, Finding> {
        let left = self.expression(0)?;
        if let Some(op) = self.peek().filter(|tok| assignment(&tok.text)) {
            let operator = op.text.clone();
            self.bump();
            let value = self.expression(0)?;
            return Ok(Statement::Assign {
                target: left,
                operator,
                value,
            });
        }
        Ok(Statement::Evaluate(left))
    }

    fn declaration(&self) -> bool {
        self.ahead(1)
            .is_some_and(|tok| tok.symbol(":=") || tok.symbol(":"))
    }

    fn variable(&mut self) -> Result<Statement, Finding> {
        self.bump();
        let mut binding = self.binding((false, false))?;
        binding.mutable = true;
        Ok(Statement::Bind(binding))
    }

    pub(crate) fn binding(&mut self, exposure: (bool, bool)) -> Result<Binding, Finding> {
        let name = self.word()?.text.clone();
        if self.peek().is_some_and(|tok| tok.symbol(":=")) {
            self.bump();
            let value = self.expression(0)?;
            return Ok(Binding {
                public: exposure.0,
                dynamic: exposure.1,
                mutable: false,
                name,
                annotation: None,
                value,
            });
        }
        self.expect(":")?;
        let annotation = Some(self.spec()?);
        self.expect("=")?;
        let value = self.expression(0)?;
        Ok(Binding {
            public: exposure.0,
            dynamic: exposure.1,
            mutable: false,
            name,
            annotation,
            value,
        })
    }

    fn iteration(&mut self, constant: bool) -> Result<Statement, Finding> {
        self.bump();
        let item = self.word()?.text.clone();
        let keyword = self.word()?;
        if keyword.text != "in" {
            return Err(keyword.fault(Code::Syntax));
        }
        let first = self.expression(0)?;
        let source = if self.peek().is_some_and(|tok| tok.symbol("..")) {
            let dots = self.bump().unwrap();
            let (line, column) = (dots.line, dots.column);
            let right = self.expression(0)?;
            Expression::Binary {
                left: Box::new(first),
                operator: "..".into(),
                right: Box::new(right),
                wrapped: false,
                line,
                column,
            }
        } else {
            first
        };
        let body = self.block()?;
        Ok(Statement::For {
            constant,
            item,
            source,
            body,
        })
    }
}

fn banned(token: &Token, second: Option<&Token>) -> Option<Code> {
    if token.kind != Kind::Word {
        return None;
    }
    match token.text.as_str() {
        "fn" if second.is_some_and(|next| next.kind == Kind::Word) => Some(Code::Signature),
        "async" if second.is_some_and(|next| next.word("fn")) => Some(Code::Signature),
        "struct" | "enum" | "interface" | "impl" | "type" => Some(Code::Model),
        "import" => Some(Code::Import),
        "dylib" => Some(Code::Boundary),
        "extern" => Some(Code::Extern),
        _ => None,
    }
}

pub(crate) fn capitalized(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn assignment(text: &str) -> bool {
    matches!(
        text,
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
    )
}
