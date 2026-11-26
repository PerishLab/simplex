use super::{Stream, capitalized};
use crate::code::{Code, Finding, Kind};
use crate::tree::{External, Import, Statement};

impl<'a> Stream<'a> {
    pub(crate) fn declared(&mut self, public: bool) -> Result<Statement, Finding> {
        if self.ahead(1).is_some_and(|tok| tok.symbol(":=")) {
            if self.ahead(2).is_some_and(|tok| tok.word("import")) {
                if public {
                    return Err(self.fault(Code::Reexport));
                }
                return self.imported();
            }
            if self.ahead(2).is_some_and(|tok| tok.word("extern")) {
                return self.external(public);
            }
        }
        Ok(Statement::Bind(self.binding(public)?))
    }

    fn imported(&mut self) -> Result<Statement, Finding> {
        let first = self.word()?;
        let (name, line, column) = (first.text.clone(), first.line, first.column);
        self.expect(":=")?;
        self.bump();
        let mut path = vec![self.segment()?];
        while self.peek().is_some_and(|tok| tok.symbol(".")) {
            self.bump();
            path.push(self.segment()?);
        }
        if path.last().is_none_or(|leaf| *leaf != name) {
            return Err(Finding {
                code: Code::Import,
                line,
                column,
            });
        }
        Ok(Statement::Import(Import { name, path }))
    }

    fn segment(&mut self) -> Result<String, Finding> {
        match self.peek() {
            Some(tok) if tok.kind == Kind::Word && !capitalized(&tok.text) => {
                Ok(self.bump().unwrap().text.clone())
            }
            _ => Err(self.fault(Code::Import)),
        }
    }

    fn external(&mut self, public: bool) -> Result<Statement, Finding> {
        let name = self.word()?.text.clone();
        self.expect(":=")?;
        let kw = self.bump().unwrap();
        let Some(abi) = self.peek().filter(|tok| tok.kind == Kind::Text) else {
            return Err(kw.fault(Code::Extern));
        };
        if abi.text != "\"c\"" {
            return Err(abi.fault(Code::Extern));
        }
        let convention = self.bump().unwrap().text.clone();
        let Some(sym) = self.peek().filter(|tok| tok.kind == Kind::Text) else {
            return Err(kw.fault(Code::Extern));
        };
        if sym.text.len() <= 2 {
            return Err(sym.fault(Code::Extern));
        }
        let symbol = self.bump().unwrap().text.clone();
        if !self.peek().is_some_and(|tok| tok.word("fn")) {
            return Err(kw.fault(Code::Extern));
        }
        self.bump();
        let inputs = self.parameters()?;
        let output = if self.peek().is_some_and(|tok| tok.symbol("->")) {
            self.bump();
            Some(self.spec()?)
        } else {
            None
        };
        Ok(Statement::Extern(External {
            public,
            name,
            abi: convention,
            symbol,
            inputs,
            output,
        }))
    }
}
