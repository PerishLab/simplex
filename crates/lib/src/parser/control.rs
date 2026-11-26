use super::Stream;
use crate::code::{Code, Finding, Kind, Token};
use crate::tree::{
    Block, Branch, Dispatch, Expression, Loop, Outcome, Parameter, Partition, Pattern, Spec,
};

impl<'a> Stream<'a> {
    pub(crate) fn repetition(&mut self, start: &Token) -> Result<Loop, Finding> {
        match self.peek() {
            Some(tok) if tok.word("switch") => {
                let kw = self.bump().unwrap();
                Ok(Loop::Switch(Box::new(self.dispatch(kw)?)))
            }
            Some(tok) if tok.word("bucket") => {
                let kw = self.bump().unwrap();
                Ok(Loop::Bucket(Box::new(self.partition(kw)?)))
            }
            Some(tok) if tok.symbol("{") => Ok(Loop::Block(self.block()?)),
            _ => Err(start.fault(Code::Syntax)),
        }
    }

    pub(crate) fn block(&mut self) -> Result<Block, Finding> {
        let open = self.expect("{")?;
        let (line, column) = (open.line, open.column);
        let compact = self.peek().is_some_and(|tok| tok.kind != Kind::Newline);
        let items = self.sequence(true)?;
        self.expect("}")?;
        if compact && items.len() > 1 {
            return Err(open.fault(Code::Format));
        }
        Ok(Block {
            items,
            compact,
            line,
            column,
        })
    }

    pub(crate) fn dispatch(&mut self, start: &Token) -> Result<Dispatch, Finding> {
        if self.peek().is_some_and(|tok| tok.symbol("{")) {
            return Err(start.fault(Code::Switch));
        }
        let subject = self.expression(0)?;
        if matches!(subject, Expression::Record { .. }) {
            return Err(start.fault(Code::Switch));
        }
        let arms = self.arms(false, Code::Switch)?;
        Ok(Dispatch {
            subject,
            arms,
            line: start.line,
            column: start.column,
        })
    }

    pub(crate) fn partition(&mut self, start: &Token) -> Result<Partition, Finding> {
        let subject = if self.peek().is_some_and(|tok| tok.symbol("{")) {
            None
        } else {
            let expr = self.expression(0)?;
            if matches!(expr, Expression::Record { .. }) {
                return Err(start.fault(Code::Bucket));
            }
            Some(expr)
        };
        let arms = self.arms(subject.is_none(), Code::Bucket)?;
        Ok(Partition {
            subject,
            arms,
            line: start.line,
            column: start.column,
        })
    }

    fn arms(&mut self, predicate: bool, empty: Code) -> Result<Vec<Branch>, Finding> {
        let open = self.expect("{")?;
        if self.newlines() == 0 {
            return Err(open.fault(Code::Format));
        }
        let mut list = Vec::new();
        while let Some(tok) = self.peek().filter(|tok| !tok.symbol("}")) {
            let (line, column) = (tok.line, tok.column);
            let pattern = self.pattern(predicate)?;
            self.expect("=>")?;
            let outcome = self.outcome()?;
            if self.peek().is_some_and(|next| next.symbol(",")) {
                return Err(self.peek().unwrap().fault(Code::Arm));
            }
            if self.newlines() == 0 && !self.peek().is_some_and(|next| next.symbol("}")) {
                return Err(self.fault(Code::Arm));
            }
            list.push(Branch {
                pattern,
                outcome,
                line,
                column,
            });
        }
        self.expect("}")?;
        if list.is_empty() {
            return Err(open.fault(empty));
        }
        Ok(list)
    }

    fn pattern(&mut self, predicate: bool) -> Result<Pattern, Finding> {
        if self.peek().is_some_and(|tok| tok.word("_"))
            && self.ahead(1).is_some_and(|tok| tok.symbol("=>"))
        {
            self.bump();
            return Ok(Pattern::Wildcard);
        }
        let first = self.expression(0)?;
        if self.peek().is_some_and(|tok| tok.symbol("..")) {
            self.bump();
            let end = self.expression(0)?;
            return Ok(Pattern::Range { start: first, end });
        }
        if predicate {
            return Ok(Pattern::Predicate(first));
        }
        let mut choices = vec![first];
        while self.peek().is_some_and(|tok| tok.symbol("|")) {
            self.bump();
            choices.push(self.expression(0)?);
        }
        Ok(Pattern::Choices(choices))
    }

    fn outcome(&mut self) -> Result<Outcome, Finding> {
        match self.peek() {
            Some(tok) if tok.word("break") => {
                self.bump();
                let value = if self.peek().is_some_and(continuable) {
                    Some(self.expression(0)?)
                } else {
                    None
                };
                Ok(Outcome::Break(value))
            }
            Some(tok) if tok.word("continue") => {
                self.bump();
                Ok(Outcome::Continue)
            }
            Some(tok) if tok.symbol("{") => Ok(Outcome::Block(self.block()?)),
            _ => Ok(Outcome::Value(self.expression(0)?)),
        }
    }

    pub(crate) fn spec(&mut self) -> Result<Spec, Finding> {
        if self.peek().is_some_and(|tok| tok.symbol("(")) {
            return Err(self.peek().unwrap().fault(Code::Tuple));
        }
        if self.peek().is_some_and(|tok| tok.word("fn")) {
            return self.signature();
        }
        let first = self.word()?.text.clone();
        let (module, name) =
            if !super::capitalized(&first) && self.peek().is_some_and(|tok| tok.symbol(".")) {
                self.bump();
                (Some(first), self.word()?.text.clone())
            } else {
                (None, first)
            };
        let mut arguments = Vec::new();
        if self.peek().is_some_and(|tok| tok.symbol("<")) {
            self.bump();
            arguments = self.speclist(">")?;
            self.expect(">")?;
        }
        Ok(Spec::Named {
            module,
            name,
            arguments,
        })
    }

    fn signature(&mut self) -> Result<Spec, Finding> {
        self.bump();
        self.expect("(")?;
        let inputs = self.speclist(")")?;
        self.expect(")")?;
        let output = if self.peek().is_some_and(|tok| tok.symbol("->")) {
            self.bump();
            Some(Box::new(self.spec()?))
        } else {
            None
        };
        Ok(Spec::Callable { inputs, output })
    }

    pub(crate) fn speclist(&mut self, stop: &str) -> Result<Vec<Spec>, Finding> {
        let mut list = Vec::new();
        while self.peek().is_some_and(|tok| !tok.symbol(stop)) {
            list.push(self.spec()?);
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(list)
    }

    pub(crate) fn parameters(&mut self) -> Result<Vec<Parameter>, Finding> {
        self.expect("(")?;
        let mut list = Vec::new();
        while self.peek().is_some_and(|tok| !tok.symbol(")")) {
            let word = self.word()?;
            let (name, line, column) = (word.text.clone(), word.line, word.column);
            let annotation = if self.peek().is_some_and(|tok| tok.symbol(":")) {
                self.bump();
                Some(self.spec()?)
            } else {
                None
            };
            list.push(Parameter {
                name,
                annotation,
                line,
                column,
            });
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(")")?;
        Ok(list)
    }
}

fn continuable(tok: &Token) -> bool {
    tok.kind != Kind::Newline && !tok.symbol("}") && !tok.symbol(",")
}
