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
    pub(crate) types: BTreeMap<String, bool>,
    pub(crate) values: BTreeMap<String, bool>,
    pub(crate) structs: BTreeMap<String, Structure>,
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
            modules.insert(imp.name.clone(), summarize(&parsed));
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
        match &item.statement {
            Statement::Struct(s) => {
                out.types.insert(s.name.clone(), s.public);
                out.structs.insert(s.name.clone(), s.clone());
            }
            Statement::Enum(e) => {
                out.types.insert(e.name.clone(), e.public);
            }
            Statement::Interface(c) => {
                out.types.insert(c.name.clone(), c.public);
            }
            Statement::Alias(a) => {
                out.types.insert(a.name.clone(), a.public);
            }
            Statement::Bind(b) => {
                out.values.insert(b.name.clone(), b.public);
            }
            Statement::Extern(ext) => {
                out.values.insert(ext.name.clone(), ext.public);
            }
            _ => {}
        }
    }
    out
}
