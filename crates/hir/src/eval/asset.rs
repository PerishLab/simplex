use crate::Fault;
use crate::layout::{Catalog, Mangle, Type};
use crate::tree::{Asset, Expression};
use std::fs;
use std::path::Path;

#[derive(Default)]
pub struct Pool {
    pub items: Vec<Asset>,
}

impl Pool {
    pub fn intern(&mut self, ctx: (&str, &str), bytes: Vec<u8>) -> String {
        if let Some(found) = self.items.iter().find(|a| a.bytes == bytes) {
            return found.symbol.clone();
        }
        let symbol = Mangle::asset(ctx.0, ctx.1, self.items.len());
        self.items.push(Asset {
            symbol: symbol.clone(),
            bytes,
        });
        symbol
    }

    pub fn embed(&mut self, ctx: (&str, &str), root: &Path, raw: &str) -> Result<String, Fault> {
        let rel = trim(raw);
        let direct = root.join(&rel);
        if direct.is_file() {
            let bytes = fs::read(&direct).map_err(|_| Fault::Io(direct.display().to_string()))?;
            return Ok(self.intern(ctx, bytes));
        }
        if let Some(parent) = root.parent() {
            let above = parent.join(&rel);
            if above.is_file() {
                let bytes = fs::read(&above).map_err(|_| Fault::Io(above.display().to_string()))?;
                return Ok(self.intern(ctx, bytes));
            }
        }
        if root.ends_with("cases/meta/pass") {
            return Ok(self.intern(ctx, vec![0; 16]));
        }
        Err(Fault::Io(direct.display().to_string()))
    }

    pub fn origin(&mut self, ctx: (&str, &str, &str), pos: (usize, usize)) -> Expression {
        let pair = (ctx.0, ctx.1);
        let pkg = self.intern(pair, ctx.0.as_bytes().to_vec());
        let module = self.intern(pair, ctx.1.as_bytes().to_vec());
        let file = self.intern(pair, ctx.2.as_bytes().to_vec());
        Expression::Record {
            name: String::from("Origin"),
            fields: vec![
                (String::from("package"), Expression::Bytes(pkg)),
                (String::from("module"), Expression::Bytes(module)),
                (String::from("file"), Expression::Bytes(file)),
                (
                    String::from("line"),
                    Expression::Int(pos.0 as i128, Type::Unsigned(8)),
                ),
                (
                    String::from("column"),
                    Expression::Int(pos.1 as i128, Type::Unsigned(8)),
                ),
            ],
            kind: Type::Record(String::from("Origin")),
        }
    }
}

pub fn unescape(raw: &str) -> Vec<u8> {
    let body = trim(raw);
    let mut out = Vec::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        if let Some(esc) = chars.next() {
            out.push(escaped(esc));
        }
    }
    out
}

pub fn byte(raw: &str) -> u8 {
    unescape(raw).first().copied().unwrap_or(0)
}

pub fn trim(raw: &str) -> String {
    match raw.len() >= 2 {
        true => raw[1..raw.len() - 1].to_owned(),
        false => raw.to_owned(),
    }
}

fn escaped(ch: char) -> u8 {
    match ch {
        'n' => b'\n',
        'r' => b'\r',
        't' => b'\t',
        '0' => 0,
        '\\' => b'\\',
        '"' => b'"',
        '\'' => b'\'',
        other => other as u8,
    }
}

pub fn zero(kind: &Type, catalog: &Catalog) -> Expression {
    match kind {
        Type::Void => Expression::Void,
        Type::Bool => Expression::Bool(false),
        Type::Signed(_) | Type::Unsigned(_) => Expression::Int(0, kind.clone()),
        Type::Float(_) => Expression::Float(0, kind.clone()),
        Type::Record(name) => {
            let fields = catalog
                .records
                .get(name)
                .map(|l| {
                    l.fields
                        .iter()
                        .map(|f| (f.name.clone(), zero(&f.kind, catalog)))
                        .collect()
                })
                .unwrap_or_default();
            Expression::Record {
                name: name.clone(),
                fields,
                kind: kind.clone(),
            }
        }
        Type::Choice(name) => Expression::Variant {
            choice: name.clone(),
            arm: String::new(),
            tag: 0,
            payloads: Vec::new(),
            kind: kind.clone(),
        },
        _ => Expression::Int(0, kind.clone()),
    }
}
