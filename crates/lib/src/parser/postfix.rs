use super::{Stream, capitalized};
use crate::code::{Code, Finding, Kind};
use crate::tree::{Expression, Field, Spec};

impl<'a> Stream<'a> {
    pub(crate) fn record(&self) -> bool {
        if !self.peek().is_some_and(|tok| tok.symbol("{")) {
            return false;
        }
        let mut look = self.index + 1;
        while self
            .tokens
            .get(look)
            .is_some_and(|tok| tok.kind == Kind::Newline)
        {
            look += 1;
        }
        let Some(first) = self.tokens.get(look) else {
            return false;
        };
        first.symbol("}")
            || (first.kind == Kind::Word
                && self.tokens.get(look + 1).is_some_and(|sec| sec.symbol(":")))
    }

    pub(crate) fn literal(
        &mut self,
        module: Option<String>,
        name: String,
        pos: (usize, usize),
    ) -> Result<Expression, Finding> {
        self.expect("{")?;
        let multiline = self.newlines() > 0;
        let mut fields = Vec::new();
        while self.peek().is_some_and(|tok| !tok.symbol("}")) {
            let field = self.word()?.text.clone();
            self.expect(":")?;
            let value = self.expression(0)?;
            fields.push(Field { name: field, value });
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                self.bump();
                self.newlines();
            } else {
                self.newlines();
                break;
            }
        }
        self.expect("}")?;
        Ok(Expression::Record {
            module,
            name,
            fields,
            multiline,
            line: pos.0,
            column: pos.1,
        })
    }

    pub(crate) fn postfix(&mut self, mut base: Expression) -> Result<Expression, Finding> {
        loop {
            if self.peek().is_some_and(|tok| tok.symbol(".")) {
                base = self.member(base)?;
            } else if self
                .peek()
                .is_some_and(|tok| tok.symbol("[") && !tok.spaced)
            {
                base = self.index(base)?;
            } else if self
                .peek()
                .is_some_and(|tok| tok.symbol("<") && !tok.spaced)
            {
                let generics = self.generics()?;
                base = self.call(base, generics)?;
            } else if self
                .peek()
                .is_some_and(|tok| tok.symbol("(") && !tok.spaced)
            {
                base = self.call(base, Vec::new())?;
            } else if self
                .peek()
                .is_some_and(|tok| tok.symbol("?") && !tok.spaced)
            {
                if self.ternary() {
                    return Err(self.peek().unwrap().fault(Code::Ternary));
                }
                self.bump();
                base = Expression::Try(Box::new(base));
            } else {
                break;
            }
        }
        Ok(base)
    }

    fn member(&mut self, base: Expression) -> Result<Expression, Finding> {
        let dot = self.bump().unwrap();
        let (line, column) = (dot.line, dot.column);
        let wrapped = self.newlines() > 0;
        let field = self.word()?.text.clone();
        let generics = self.generics()?;
        if !wrapped
            && generics.is_empty()
            && let Some((owner, origin)) = qualified(&base, &field, self.record())
        {
            return self.literal(Some(owner), field, origin);
        }
        Ok(Expression::Member {
            base: Box::new(base),
            field,
            generics,
            wrapped,
            line,
            column,
        })
    }

    fn ternary(&self) -> bool {
        for tok in &self.tokens[self.index + 1..] {
            if tok.kind == Kind::Newline {
                return false;
            }
            if tok.symbol(":") {
                return true;
            }
        }
        false
    }

    fn generics(&mut self) -> Result<Vec<Spec>, Finding> {
        let mut list = Vec::new();
        if self
            .peek()
            .is_some_and(|tok| tok.symbol("<") && !tok.spaced)
        {
            self.bump();
            list = self.speclist(">")?;
            self.expect(">")?;
        }
        Ok(list)
    }

    fn index(&mut self, base: Expression) -> Result<Expression, Finding> {
        let open = self.expect("[")?;
        let (line, column) = (open.line, open.column);
        let start = self.expression(0)?;
        let limit = if self.peek().is_some_and(|tok| tok.symbol("..")) {
            self.bump();
            Some(Box::new(self.expression(0)?))
        } else {
            None
        };
        self.expect("]")?;
        Ok(Expression::Index {
            base: Box::new(base),
            start: Box::new(start),
            limit,
            line,
            column,
        })
    }

    fn call(&mut self, callee: Expression, generics: Vec<Spec>) -> Result<Expression, Finding> {
        let open = self.expect("(")?;
        let (line, column) = (open.line, open.column);
        let multiline = self.newlines() > 0;
        let mut arguments = Vec::new();
        while self.peek().is_some_and(|tok| !tok.symbol(")")) {
            arguments.push(self.expression(0)?);
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                self.bump();
                self.newlines();
            } else {
                self.newlines();
                break;
            }
        }
        self.expect(")")?;
        Ok(Expression::Call {
            callee: Box::new(callee),
            generics,
            arguments,
            multiline,
            line,
            column,
        })
    }
}

fn qualified(base: &Expression, field: &str, braced: bool) -> Option<(String, (usize, usize))> {
    if !braced || !capitalized(field) {
        return None;
    }
    let Expression::Word { text, line, column } = base else {
        return None;
    };
    (!capitalized(text)).then(|| (text.clone(), (*line, *column)))
}
