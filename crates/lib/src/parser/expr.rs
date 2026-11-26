use super::{Stream, capitalized};
use crate::code::{Code, Finding, Kind, Token};
use crate::tree::{Body, Expression, Generic};

impl<'a> Stream<'a> {
    pub(crate) fn expression(&mut self, minimum: u8) -> Result<Expression, Finding> {
        let mut left = self.prefix()?;
        left = self.postfix(left)?;
        while let Some((power, operator)) = self.infix() {
            if power < minimum {
                break;
            }
            let op = self.bump().unwrap();
            let (line, column) = (op.line, op.column);
            let wrapped = self.newlines() > 0;
            let right = self.expression(power + 1)?;
            left = Expression::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
                wrapped,
                line,
                column,
            };
        }
        if self.peek().is_some_and(|tok| tok.symbol("?")) {
            return Err(self.peek().unwrap().fault(Code::Ternary));
        }
        Ok(left)
    }

    fn infix(&self) -> Option<(u8, String)> {
        let tok = self.peek()?;
        if tok.kind != Kind::Symbol {
            return None;
        }
        let power = match tok.text.as_str() {
            "||" => 1,
            "&&" => 2,
            "==" | "!=" | "<" | "<=" | ">" | ">=" => 3,
            "|" => return None,
            "^" => 4,
            "&" => 5,
            "<<" | ">>" => 6,
            "+" | "-" => 7,
            "*" | "/" | "%" => 8,
            _ => return None,
        };
        Some((power, tok.text.clone()))
    }

    pub(crate) fn prefix(&mut self) -> Result<Expression, Finding> {
        let Some(tok) = self.peek() else {
            return Err(self.fault(Code::Syntax));
        };
        if tok.word("inline") {
            return Err(tok.fault(Code::Inline));
        }
        if tok.word("fn") {
            return Err(tok.fault(Code::Signature));
        }
        if tok.word("switch") {
            let start = self.bump().unwrap();
            return Ok(Expression::Switch(Box::new(self.dispatch(start)?)));
        }
        if tok.word("bucket") {
            let start = self.bump().unwrap();
            return Ok(Expression::Bucket(Box::new(self.partition(start)?)));
        }
        if tok.word("loop") {
            let start = self.bump().unwrap();
            return Ok(Expression::Loop(Box::new(self.repetition(start)?)));
        }
        if tok.symbol("<") {
            let generics = self.clauses()?;
            return self.function(generics);
        }
        if tok.symbol("(") {
            return if self.callable() {
                self.function(Vec::new())
            } else {
                self.grouped()
            };
        }
        if tok.symbol("-") || tok.symbol("!") {
            let operator = self.bump().unwrap().text.clone();
            let operand = self.prefix()?;
            let full = self.postfix(operand)?;
            return Ok(Expression::Unary {
                operator,
                operand: Box::new(full),
            });
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expression, Finding> {
        let Some(tok) = self.bump() else {
            return Err(self.fault(Code::Syntax));
        };
        match tok.kind {
            Kind::Number => Ok(Expression::Number(tok.text.clone())),
            Kind::Text => Ok(Expression::Text(tok.text.clone())),
            Kind::Byte => Ok(Expression::Byte(tok.text.clone())),
            Kind::Word => {
                if capitalized(&tok.text) && self.record() {
                    self.literal(tok.text.clone())
                } else {
                    Ok(Expression::Word {
                        text: tok.text.clone(),
                        line: tok.line,
                        column: tok.column,
                    })
                }
            }
            _ => Err(tok.fault(Code::Syntax)),
        }
    }

    fn grouped(&mut self) -> Result<Expression, Finding> {
        self.expect("(")?;
        let inner = self.expression(0)?;
        self.expect(")")?;
        Ok(Expression::Group(Box::new(inner)))
    }

    fn function(&mut self, generics: Vec<Generic>) -> Result<Expression, Finding> {
        let inputs = self.parameters()?;
        let output = if self.peek().is_some_and(|tok| tok.symbol("->")) {
            self.bump();
            Some(self.spec()?)
        } else {
            None
        };
        let Some(next) = self.peek() else {
            return Err(self.fault(Code::Inline));
        };
        let body = match next.text.as_str() {
            "{" if next.kind == Kind::Symbol => Body::Block(self.block()?),
            "switch" if next.kind == Kind::Word => {
                let kw = self.bump().unwrap();
                Body::Switch(Box::new(self.dispatch(kw)?))
            }
            "bucket" if next.kind == Kind::Word => {
                let kw = self.bump().unwrap();
                Body::Bucket(Box::new(self.partition(kw)?))
            }
            "loop" if next.kind == Kind::Word => {
                let kw = self.bump().unwrap();
                Body::Loop(Box::new(self.repetition(kw)?))
            }
            _ => return Err(next.fault(Code::Inline)),
        };
        Ok(Expression::Function {
            generics,
            inputs,
            output,
            body,
        })
    }

    fn callable(&self) -> bool {
        let Some(close) = self.closing() else {
            return false;
        };
        let inner = &self.tokens[self.index + 1..close];
        if inner.is_empty() || typed(inner) {
            return true;
        }
        inner.len() == 1
            && inner[0].kind == Kind::Word
            && self.tokens.get(close + 1).is_some_and(trigger)
    }

    fn closing(&self) -> Option<usize> {
        let mut depth = 0usize;
        for (offset, tok) in self.tokens[self.index..].iter().enumerate() {
            if tok.symbol("(") {
                depth += 1;
            } else if tok.symbol(")") {
                depth -= 1;
                if depth == 0 {
                    return Some(self.index + offset);
                }
            }
        }
        None
    }
}

fn typed(inner: &[Token]) -> bool {
    inner[0].kind == Kind::Word
        && inner
            .get(1)
            .is_some_and(|tok| tok.symbol(":") || tok.symbol(","))
}

fn trigger(after: &Token) -> bool {
    matches!(
        (after.kind, after.text.as_str()),
        (Kind::Symbol, "{" | "->" | "=>") | (Kind::Word, "switch" | "bucket" | "loop" | "for")
    )
}
