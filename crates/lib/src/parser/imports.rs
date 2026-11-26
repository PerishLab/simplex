use super::{Stream, capitalized};
use crate::code::{Code, Finding, Kind};
use crate::tree::{External, Generic, Import, Statement};

impl<'a> Stream<'a> {
    pub(crate) fn declared(&mut self, exposure: (bool, bool)) -> Result<Statement, Finding> {
        if !self.ahead(1).is_some_and(|tok| tok.symbol(":=")) {
            return Ok(Statement::Bind(self.binding(exposure)?));
        }
        if self.ahead(2).is_some_and(|tok| tok.word("extern")) {
            return self.external(exposure);
        }
        if !self
            .ahead(2)
            .is_some_and(|tok| tok.word("import") || tok.word("dylib"))
        {
            return Ok(Statement::Bind(self.binding(exposure)?));
        }
        let dynamic = self.ahead(2).is_some_and(|tok| tok.word("dylib"));
        if exposure.0 {
            let code = if dynamic {
                Code::Boundary
            } else {
                Code::Reexport
            };
            return Err(self.fault(code));
        }
        self.imported(dynamic)
    }

    fn imported(&mut self, dynamic: bool) -> Result<Statement, Finding> {
        let first = self.word()?;
        let (name, line, column) = (first.text.clone(), first.line, first.column);
        self.expect(":=")?;
        self.bump();
        let fault = if dynamic {
            Code::Boundary
        } else {
            Code::Import
        };
        let mut path = vec![self.segment(fault)?];
        while self.peek().is_some_and(|tok| tok.symbol(".")) {
            self.bump();
            path.push(self.segment(fault)?);
        }
        if path.last().is_none_or(|leaf| *leaf != name) {
            return Err(Finding {
                code: fault,
                line,
                column,
            });
        }
        Ok(Statement::Import(Import {
            dynamic,
            name,
            path,
        }))
    }

    fn segment(&mut self, fault: Code) -> Result<String, Finding> {
        match self.peek() {
            Some(tok) if tok.kind == Kind::Word && !capitalized(&tok.text) => {
                Ok(self.bump().unwrap().text.clone())
            }
            _ => Err(self.fault(fault)),
        }
    }

    fn external(&mut self, exposure: (bool, bool)) -> Result<Statement, Finding> {
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
            public: exposure.0,
            dynamic: exposure.1,
            name,
            abi: convention,
            symbol,
            inputs,
            output,
        }))
    }

    pub(crate) fn clauses(&mut self) -> Result<Vec<Generic>, Finding> {
        self.expect("<")?;
        let mut list = Vec::new();
        while self.peek().is_some_and(|tok| !tok.symbol(">")) {
            let name = self.word()?.text.clone();
            let mut bounds = Vec::new();
            if self.peek().is_some_and(|tok| tok.symbol(":")) {
                self.bump();
                bounds.push(self.word()?.text.clone());
                while self.peek().is_some_and(|tok| tok.symbol("+")) {
                    self.bump();
                    bounds.push(self.word()?.text.clone());
                }
            }
            list.push(Generic { name, bounds });
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(">")?;
        Ok(list)
    }
}
