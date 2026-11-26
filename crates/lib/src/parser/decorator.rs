use super::{Stream, capitalized};
use crate::code::{Code, Finding, Kind, Token};
use crate::tree::{Decorator, Expression, Statement};

impl<'a> Stream<'a> {
    pub(crate) fn decorated(&mut self, braced: bool) -> Result<Statement, Finding> {
        if braced {
            return Err(self.fault(Code::Placement));
        }
        let list = self.decorators()?;
        let first = &list[0];
        let pos = (first.line, first.column);
        let stmt = self.statement(false)?;
        attach(stmt, list, pos)
    }

    pub(crate) fn decorators(&mut self) -> Result<Vec<Decorator>, Finding> {
        let mut list = Vec::new();
        while self.peek().is_some_and(|tok| tok.symbol("@")) {
            list.push(self.decorator()?);
        }
        Ok(list)
    }

    fn decorator(&mut self) -> Result<Decorator, Finding> {
        let mark = self.bump().unwrap();
        let head = self.selector(mark)?;
        let dotted = self
            .peek()
            .is_some_and(|tok| tok.symbol(".") && !tok.spaced);
        if !dotted {
            match head.as_str() {
                "test" => return Err(mark.fault(Code::Test)),
                "cfg" => return Err(mark.fault(Code::Condition)),
                _ => {}
            }
        }
        let (module, name) = if dotted {
            self.bump();
            (Some(head), self.selector(mark)?)
        } else {
            (None, head)
        };
        let arguments = if self
            .peek()
            .is_some_and(|tok| tok.symbol("(") && !tok.spaced)
        {
            self.supplied()?
        } else {
            Vec::new()
        };
        if self.newlines() != 1 {
            return Err(self.fault(Code::Format));
        }
        if self.peek().is_none_or(|tok| tok.symbol("}")) {
            return Err(mark.fault(Code::Placement));
        }
        Ok(Decorator {
            module,
            name,
            arguments,
            line: mark.line,
            column: mark.column,
        })
    }

    fn selector(&mut self, mark: &Token) -> Result<String, Finding> {
        let Some(tok) = self
            .peek()
            .filter(|tok| tok.kind == Kind::Word && !tok.spaced && !capitalized(&tok.text))
        else {
            return Err(mark.fault(Code::Placement));
        };
        let text = tok.text.clone();
        self.bump();
        Ok(text)
    }

    fn supplied(&mut self) -> Result<Vec<Expression>, Finding> {
        self.expect("(")?;
        let mut items = Vec::new();
        while self.peek().is_some_and(|tok| !tok.symbol(")")) {
            items.push(self.expression(0)?);
            if self.peek().is_some_and(|tok| tok.symbol(",")) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(")")?;
        Ok(items)
    }
}

fn attach(
    stmt: Statement,
    list: Vec<Decorator>,
    pos: (usize, usize),
) -> Result<Statement, Finding> {
    match stmt {
        Statement::Bind(mut b) => {
            b.decorators = list;
            Ok(Statement::Bind(b))
        }
        Statement::Struct(mut s) => {
            s.decorators = list;
            Ok(Statement::Struct(s))
        }
        Statement::Enum(mut e) => {
            e.decorators = list;
            Ok(Statement::Enum(e))
        }
        _ => Err(Finding {
            code: Code::Placement,
            line: pos.0,
            column: pos.1,
        }),
    }
}
