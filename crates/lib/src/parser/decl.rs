use super::{Stream, capitalized};
use crate::code::{Code, Finding, Kind, Token};
use crate::tree::{
    Alias, Conformance, Contract, Entry, Enumeration, Generic, Requirement, Slot, Statement,
    Structure, Variant,
};

impl<'a> Stream<'a> {
    pub(crate) fn nominal(&mut self) -> Result<Option<Statement>, Finding> {
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
        if self.peek().is_some_and(|tok| tok.symbol(":")) && generics.is_empty() {
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
        match self.peek() {
            Some(tok) if tok.word("struct") => self.structure(name, generics).map(Some),
            Some(tok) if tok.word("enum") => self.enumeration(name, generics).map(Some),
            Some(tok) if tok.word("interface") => self.contract(name, generics).map(Some),
            Some(tok) if tok.kind == Kind::Word && capitalized(&tok.text) && self.aliased() => {
                let target = self.spec()?;
                Ok(Some(Statement::Alias(Alias {
                    name,
                    generics,
                    target,
                })))
            }
            _ if generics.is_empty() => {
                self.index = saved;
                Ok(None)
            }
            _ => Err(self.fault(Code::Model)),
        }
    }

    fn aliased(&self) -> bool {
        let look = skip(self.tokens, self.index + 1);
        self.tokens
            .get(look)
            .is_none_or(|tok| tok.kind == Kind::Newline)
    }

    fn conformance(
        &mut self,
        subject: String,
        contract: crate::tree::Spec,
    ) -> Result<Statement, Finding> {
        let open = self.expect("{")?;
        let mut methods = Vec::new();
        if self.peek().is_some_and(|tok| tok.symbol("}")) {
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
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                return Err(self.peek().unwrap().fault(Code::Model));
            }
            if self.newlines() == 0 && !self.peek().is_some_and(closes) {
                return Err(self.fault(Code::Model));
            }
            methods.push(Entry { name, value });
        }
        self.expect("}")?;
        Ok(Statement::Conform(Conformance {
            subject,
            contract,
            methods,
        }))
    }

    fn structure(&mut self, name: String, generics: Vec<Generic>) -> Result<Statement, Finding> {
        self.bump();
        if self.peek().is_some_and(|tok| tok.word("extends")) {
            return Err(self.peek().unwrap().fault(Code::Inheritance));
        }
        let open = self.expect("{")?;
        let mut fields = Vec::new();
        if self.peek().is_some_and(|tok| tok.symbol("}")) {
            self.bump();
            return Ok(Statement::Struct(Structure {
                name,
                generics,
                fields,
            }));
        }
        if self.newlines() == 0 {
            return Err(open.fault(Code::Format));
        }
        while self.peek().is_some_and(|tok| !tok.symbol("}")) {
            let field = self.word()?.text.clone();
            self.expect(":")?;
            let spec = self.spec()?;
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                return Err(self.peek().unwrap().fault(Code::Model));
            }
            if self.newlines() == 0 && !self.peek().is_some_and(closes) {
                return Err(self.fault(Code::Model));
            }
            fields.push(Slot { name: field, spec });
        }
        self.expect("}")?;
        Ok(Statement::Struct(Structure {
            name,
            generics,
            fields,
        }))
    }

    fn enumeration(&mut self, name: String, generics: Vec<Generic>) -> Result<Statement, Finding> {
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
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                return Err(self.peek().unwrap().fault(Code::Model));
            }
            if self.newlines() == 0 && !self.peek().is_some_and(closes) {
                return Err(self.fault(Code::Model));
            }
            variants.push(Variant {
                name: label,
                payloads,
            });
        }
        self.expect("}")?;
        Ok(Statement::Enum(Enumeration {
            name,
            generics,
            variants,
        }))
    }

    fn contract(&mut self, name: String, generics: Vec<Generic>) -> Result<Statement, Finding> {
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
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                return Err(self.peek().unwrap().fault(Code::Model));
            }
            if self.newlines() == 0 && !self.peek().is_some_and(closes) {
                return Err(self.fault(Code::Model));
            }
            methods.push(Requirement {
                name: method,
                inputs,
                output,
            });
        }
        self.expect("}")?;
        Ok(Statement::Interface(Contract {
            name,
            generics,
            methods,
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
