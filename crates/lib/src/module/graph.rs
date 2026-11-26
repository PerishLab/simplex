use super::access::Access;
use crate::code::{Code, Finding};
use crate::lexer::{surface, tokenize};
use crate::parser;
use crate::tree::{Import, Program, Statement, Structure};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct Exports {
    pub(crate) linked: bool,
    pub(crate) types: BTreeMap<String, bool>,
    pub(crate) values: BTreeMap<String, bool>,
    pub(crate) shared: BTreeSet<String>,
    pub(crate) structs: BTreeMap<String, Structure>,
}

impl Exports {
    pub(crate) fn value(&self, field: &str) -> Option<Code> {
        if self.linked {
            return (!self.shared.contains(field)).then_some(Code::Boundary);
        }
        let visible = self.values.get(field).copied().unwrap_or(false)
            || self.types.get(field).copied().unwrap_or(false);
        (!visible).then_some(Code::Visibility)
    }

    pub(crate) fn kind(&self, name: &str) -> Option<Code> {
        if self.linked {
            return (!self.shared.contains(name)).then_some(Code::Boundary);
        }
        (!self.types.get(name).copied().unwrap_or(false)).then_some(Code::Visibility)
    }

    pub(crate) fn constructible(&self, name: &str) -> Option<Code> {
        if self.linked && !self.shared.contains(name) {
            return Some(Code::Boundary);
        }
        let ok = self
            .structs
            .get(name)
            .is_some_and(|s| s.public && s.fields.iter().all(|slot| slot.public));
        (!ok).then_some(Code::Visibility)
    }

    pub(crate) fn readable(&self, kind: &str, field: &str) -> bool {
        let Some(structure) = self.structs.get(kind) else {
            return true;
        };
        structure
            .fields
            .iter()
            .find(|slot| slot.name == field)
            .is_none_or(|slot| slot.public)
    }

    fn expose(&mut self, name: &str, shared: bool) {
        if shared {
            self.shared.insert(name.to_string());
        }
    }

    fn record(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Struct(s) => {
                self.types.insert(s.name.clone(), s.public);
                self.structs.insert(s.name.clone(), s.clone());
                self.expose(&s.name, s.dynamic);
            }
            Statement::Enum(e) => {
                self.types.insert(e.name.clone(), e.public);
                self.expose(&e.name, e.dynamic);
            }
            Statement::Interface(c) => {
                self.types.insert(c.name.clone(), c.public);
                self.expose(&c.name, c.dynamic);
            }
            Statement::Alias(a) => {
                self.types.insert(a.name.clone(), a.public);
                self.expose(&a.name, a.dynamic);
            }
            Statement::Bind(b) => {
                self.values.insert(b.name.clone(), b.public);
                self.expose(&b.name, b.dynamic);
            }
            Statement::Extern(ext) => {
                self.values.insert(ext.name.clone(), ext.public);
                self.expose(&ext.name, ext.dynamic);
            }
            _ => {}
        }
    }
}

pub(crate) fn verify(file: &str, dir: &Path, program: &Program) -> Result<(), Finding> {
    let mut stack = BTreeSet::new();
    stack.insert(dir.join(file));
    let modules = load(dir, program, &mut stack)?;
    Access::new(&modules).items(&program.items)
}

fn load(
    dir: &Path,
    program: &Program,
    stack: &mut BTreeSet<PathBuf>,
) -> Result<BTreeMap<String, Exports>, Finding> {
    let mut modules = BTreeMap::new();
    for item in &program.items {
        let Statement::Import(imp) = &item.statement else {
            continue;
        };
        if let Some(target) = locate(dir, imp) {
            if !stack.insert(target.clone()) {
                return Err(Finding {
                    code: Code::Cycle,
                    line: item.line,
                    column: item.column,
                });
            }
            let parsed = read(&target, (item.line, item.column))?;
            let folder = target.parent().unwrap_or(dir);
            load(folder, &parsed, stack)?;
            stack.remove(&target);
            let mut summary = summarize(&parsed);
            summary.linked = imp.dynamic;
            modules.insert(imp.name.clone(), summary);
        }
    }
    Ok(modules)
}

fn locate(dir: &Path, imp: &Import) -> Option<PathBuf> {
    let mut rel = PathBuf::new();
    for seg in &imp.path {
        rel.push(seg);
    }
    rel.set_extension("sx");
    let direct = dir.join(&rel);
    if direct.is_file() {
        return Some(direct);
    }
    let sibling = dir.parent()?.join("pass").join(&rel);
    sibling.is_file().then_some(sibling)
}

fn read(path: &Path, pos: (usize, usize)) -> Result<Program, Finding> {
    let Ok(source) = fs::read_to_string(path) else {
        return Err(Finding {
            code: Code::Import,
            line: pos.0,
            column: pos.1,
        });
    };
    let tokens = tokenize(&source)?;
    surface::check(&tokens)?;
    parser::parse(&tokens)
}

fn summarize(program: &Program) -> Exports {
    let mut out = Exports::default();
    for item in &program.items {
        out.record(&item.statement);
    }
    out
}
