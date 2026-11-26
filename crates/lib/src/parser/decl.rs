use super::Stream;
use crate::code::{Code, Finding, Kind, Token};
use crate::tree::{
    Alias, Conformance, Contract, Entry, Enumeration, Generic, Requirement, Slot, Spec, Statement,
    Structure, Variant,
};

impl<'a> Stream<'a> {
    pub(crate) fn nominal(&mut self, public: bool) -> Result<Option<Statement>, Finding> {
        if self.ahead(1).is_some_and(|tok| tok.symbol("."))
            && self.ahead(2).is_some_and(|tok| tok.kind == Kind::Word)
            && self.ahead(3).is_some_and(|tok| tok.symbol(":="))
        {
            return Err(self.peek().unwrap().fault(Code::Model));
        }
        let saved = self.index;
        let name = self.word()?.text.clone();
        let generics = if self
            .peek()
            .is_some_and(|tok| tok.symbol("<") && !tok.spaced)
        {
            self.clauses()?
        } else {
            Vec::new()
        };
        if !public && self.peek().is_some_and(|tok| tok.symbol(":")) && generics.is_empty() {
            self.bump();
            let contract = self.spec()?;
            if !self.peek().is_some_and(|tok| tok.symbol("=")) {
                return self.conformance(name, contract).map(Some);
            }
            self.index = saved;
            return Ok(None);
        }
        if !self.peek().is_some_and(|tok| tok.symbol(":=")) {
            self.index = saved;
            return Ok(None);
        }
        self.bump();
        self.definition(public, name, generics)
    }

    fn definition(
        &mut self,
        public: bool,
        name: String,
        generics: Vec<Generic>,
    ) -> Result<Option<Statement>, Finding> {
        match self.peek() {
            Some(tok) if tok.word("struct") => self.structure(public, name, generics).map(Some),
            Some(tok) if tok.word("enum") => self.enumeration(public, name, generics).map(Some),
            Some(tok) if tok.word("interface") => self.contract(public, name, generics).map(Some),
            Some(tok) if tok.kind == Kind::Word && self.aliased() => {
                let target = self.spec()?;
                Ok(Some(Statement::Alias(Alias {
                    public,
                    name,
                    generics,
                    target,
                })))
            }
            _ => Err(self.fault(Code::Model)),
        }
    }

    fn aliased(&self) -> bool {
        let start = if self.ahead(1).is_some_and(|tok| tok.symbol(".")) {
            self.index + 3
        } else {
            self.index + 1
        };
        let look = skip(self.tokens, start);
        self.tokens
            .get(look)
            .is_none_or(|tok| tok.kind == Kind::Newline)
    }

    fn conformance(&mut self, subject: String, contract: Spec) -> Result<Statement, Finding> {
        let open = self.expect("{")?;
        let mut methods = Vec::new();
        if self.peek().is_some_and(closes) {
            self.bump();
            return Ok(Statement::Conform(Conformance {
                subject,
                contract,
                methods,
            }));
        }
        if self.newlines() == 0 {
            return Err(open.fault(Code::Format));
        }
        while self.peek().is_some_and(|tok| !tok.symbol("}")) {
            let name = self.word()?.text.clone();
            self.expect(":")?;
            let value = self.expression(0)?;
            self.delimit()?;
            methods.push(Entry { name, value });
        }
        self.expect("}")?;
        Ok(Statement::Conform(Conformance {
            subject,
            contract,
            methods,
        }))
    }

    fn structure(
        &mut self,
        public: bool,
        name: String,
        generics: Vec<Generic>,
    ) -> Result<Statement, Finding> {
        self.bump();
        if self.peek().is_some_and(|tok| tok.word("extends")) {
            return Err(self.peek().unwrap().fault(Code::Inheritance));
        }
        let open = self.expect("{")?;
        let mut fields = Vec::new();
        if self.peek().is_some_and(closes) {
            self.bump();
            return Ok(Statement::Struct(Structure {
                public,
                name,
                generics,
                fields,
            }));
        }
        if self.newlines() == 0 {
            return Err(open.fault(Code::Format));
        }
        while self.peek().is_some_and(|tok| !tok.symbol("}")) {
            let exposed = if self.peek().is_some_and(|tok| tok.word("pub")) {
                self.bump();
                true
            } else {
                false
            };
            let field = self.word()?.text.clone();
            self.expect(":")?;
            let spec = self.spec()?;
            self.delimit()?;
            fields.push(Slot {
                public: exposed,
                name: field,
                spec,
            });
        }
        self.expect("}")?;
        Ok(Statement::Struct(Structure {
            public,
            name,
            generics,
            fields,
        }))
    }

    fn enumeration(
        &mut self,
        public: bool,
        name: String,
        generics: Vec<Generic>,
    ) -> Result<Statement, Finding> {
        self.bump();
        let open = self.expect("{")?;
        if self.newlines() == 0 {
            return Err(open.fault(Code::Format));
        }
        let mut variants = Vec::new();
        while self.peek().is_some_and(|tok| !tok.symbol("}")) {
            let label = self.word()?.text.clone();
            let payloads = if self.peek().is_some_and(|tok| tok.symbol("(")) {
                self.bump();
                let list = self.speclist(")")?;
                self.expect(")")?;
                list
            } else {
                Vec::new()
            };
            self.delimit()?;
            variants.push(Variant {
                name: label,
                payloads,
            });
        }
        self.expect("}")?;
        Ok(Statement::Enum(Enumeration {
            public,
            name,
            generics,
            variants,
        }))
    }

    fn contract(
        &mut self,
        public: bool,
        name: String,
        generics: Vec<Generic>,
    ) -> Result<Statement, Finding> {
        self.bump();
        let open = self.expect("{")?;
        if self.newlines() == 0 {
            return Err(open.fault(Code::Format));
        }
        let mut methods = Vec::new();
        while self.peek().is_some_and(|tok| !tok.symbol("}")) {
            let method = self.word()?.text.clone();
            self.expect(":")?;
            let inputs = self.parameters()?;
            let output = if self.peek().is_some_and(|tok| tok.symbol("->")) {
                self.bump();
                Some(self.spec()?)
            } else {
                None
            };
            self.delimit()?;
            methods.push(Requirement {
                name: method,
                inputs,
                output,
            });
        }
        self.expect("}")?;
        Ok(Statement::Interface(Contract {
            public,
            name,
            generics,
            methods,
        }))
    }

    fn delimit(&mut self) -> Result<(), Finding> {
        if self.peek().is_some_and(|tok| tok.symbol(",")) {
            return Err(self.peek().unwrap().fault(Code::Model));
        }
        if self.newlines() == 0 && !self.peek().is_some_and(closes) {
            return Err(self.fault(Code::Model));
        }
        Ok(())
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

fn closes(tok: &Token) -> bool {
    tok.symbol("}")
}

fn skip(tokens: &[Token], start: usize) -> usize {
    if !tokens.get(start).is_some_and(|tok| tok.symbol("<")) {
        return start;
    }
    let mut depth = 0usize;
    for (offset, tok) in tokens[start..].iter().enumerate() {
        if tok.symbol("<") {
            depth += 1;
        } else if tok.symbol(">") {
            depth -= 1;
            if depth == 0 {
                return start + offset + 1;
            }
        }
    }
    tokens.len()
}
