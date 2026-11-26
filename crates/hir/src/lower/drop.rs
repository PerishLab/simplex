use crate::layout::{Field, Mangle, Type};
use crate::lower::engine::Engine;
use crate::lower::frame::{Frame, Local};
use crate::tree::{Expression, Routine, Statement};

impl<'a> Engine<'a> {
    pub fn dropper(&mut self, stem: &str, kind: &Type) -> Option<(String, bool)> {
        let key = match kind {
            Type::Record(k) | Type::Choice(k) => k.clone(),
            _ => return None,
        };
        if let Some(cached) = self.drops.get(&key) {
            return cached.clone();
        }
        self.drops.insert(key.clone(), None);
        let found = self.destructor(stem, &key, kind);
        self.drops.insert(key, found.clone());
        found
    }

    fn destructor(&mut self, stem: &str, key: &str, kind: &Type) -> Option<(String, bool)> {
        let (owner, raw, args) = self
            .typing
            .owner(key)
            .cloned()
            .unwrap_or_else(|| (stem.to_owned(), key.to_owned(), Vec::new()));
        let claims = &self.world.claims;
        if let Some(claim) = claims
            .iter()
            .find(|c| c.subject == raw && c.target.1 == "Drop")
            .cloned()
        {
            let tsym = self.table(&claim, &args);
            let msym = Mangle::method(&tsym, "drop");
            let borrowed = self
                .routines
                .iter()
                .find(|r| r.symbol == msym)
                .and_then(|r| r.inputs.first())
                .is_none_or(|(_, t)| matches!(t, Type::Pointer(_)));
            return Some((msym, borrowed));
        }
        let layout = self.typing.catalog.records.get(key).cloned()?;
        let mut steps = Vec::new();
        for field in layout.fields.iter().rev() {
            if let Some((fsym, fborrow)) = self.dropper(&owner, &field.kind) {
                steps.push(glue(kind, field, fsym, fborrow));
            }
        }
        if steps.is_empty() {
            return None;
        }
        let gsym = format!("sx$impl${}${}${key}$Drop$glue", self.ns, owner);
        if self.seen.insert(gsym.clone()) {
            self.routines.push(Routine {
                symbol: gsym.clone(),
                exported: false,
                asynchronous: false,
                inputs: vec![(String::from("self"), Type::Pointer(Box::new(kind.clone())))],
                output: Type::Void,
                body: steps,
            });
        }
        Some((gsym, true))
    }

    pub fn copyable(&self, stem: &str, kind: &Type) -> bool {
        match kind {
            Type::Void | Type::Bool | Type::Signed(_) | Type::Unsigned(_) | Type::Float(_) => true,
            Type::Slice(_) | Type::Trait(_) | Type::Function { .. } | Type::Pointer(_) => true,
            Type::List(_) | Type::Fiber(_) => false,
            Type::Record(key) | Type::Choice(key) => {
                let (owner, raw, _) = self
                    .typing
                    .owner(key)
                    .cloned()
                    .unwrap_or_else(|| (stem.to_owned(), key.clone(), Vec::new()));
                self.world.copies.contains(&(owner, raw))
            }
        }
    }
}

impl<'b, 'a> Frame<'b, 'a> {
    pub fn push(&mut self) {
        self.scopes.push(Vec::new());
    }

    pub fn pop(&mut self, out: &mut Vec<Statement>) {
        let Some(frame) = self.scopes.pop() else {
            return;
        };
        for slot in frame.into_iter().rev() {
            if !slot.moved
                && let Some((symbol, borrowed)) = slot.dropper
            {
                out.push(Statement::Drop {
                    name: slot.name,
                    symbol,
                    borrowed,
                });
            }
        }
    }

    pub fn unwind(&self, floor: usize) -> Vec<Statement> {
        let mut out = Vec::new();
        for slot in self.scopes[floor..]
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
        {
            if !slot.moved
                && let Some((symbol, borrowed)) = &slot.dropper
            {
                out.push(Statement::Drop {
                    name: slot.name.clone(),
                    symbol: symbol.clone(),
                    borrowed: *borrowed,
                });
            }
        }
        out
    }

    pub fn declare(&mut self, name: &str, kind: Type) {
        let dropper = self.engine.dropper(&self.stem, &kind);
        if let Some(top) = self.scopes.last_mut() {
            top.push(Local {
                name: name.to_owned(),
                kind,
                dropper,
                moved: false,
            });
        }
    }

    pub fn jump(&self, word: &str, out: &mut Vec<Statement>) -> bool {
        if !matches!(word, "continue" | "break") {
            return false;
        }
        let floor = self.loops.last().copied().unwrap_or(0);
        out.extend(self.unwind(floor));
        match word {
            "continue" => out.push(Statement::Continue),
            _ => out.push(Statement::Break(None)),
        }
        true
    }
}

fn glue(kind: &Type, field: &Field, symbol: String, borrowed: bool) -> Statement {
    let base = Expression::Load {
        pointer: Box::new(Expression::Local(
            String::from("self"),
            Type::Pointer(Box::new(kind.clone())),
        )),
        kind: kind.clone(),
    };
    let proj = Expression::Field {
        base: Box::new(base),
        name: field.name.clone(),
        offset: field.offset,
        kind: field.kind.clone(),
    };
    let arg = match borrowed {
        true => Expression::Borrow {
            mutable: true,
            target: Box::new(proj),
            kind: Type::Pointer(Box::new(field.kind.clone())),
        },
        false => proj,
    };
    Statement::Evaluate(Expression::Call {
        symbol,
        arguments: vec![arg],
        kind: Type::Void,
    })
}
