use super::encode::Encoder;
use super::lower::{Emitter, shape};
use super::{Flavor, Register, Width};
use crate::Fault;
use simplex_ir::{Function, Linkage, Module, Operation, Type};
use simplex_object::{Fixup, Import, Relocation, Symbol};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct Catalog {
    pub(crate) defined: BTreeSet<String>,
    pub(crate) mapped: BTreeMap<String, (String, String)>,
}

impl Catalog {
    pub(crate) fn seed(&mut self, module: &Module) {
        for func in &module.functions {
            self.defined.insert(func.symbol.clone());
        }
        for asset in &module.assets {
            self.defined.insert(asset.symbol.clone());
        }
        for table in &module.tables {
            self.defined.insert(table.symbol.clone());
        }
    }

    pub(crate) fn resolve(&self, symbol: &str) -> (String, bool) {
        if self.defined.contains(symbol) {
            return (symbol.to_owned(), false);
        }
        if let Some((target, _)) = self.mapped.get(symbol) {
            return (target.clone(), true);
        }
        (symbol.to_owned(), false)
    }

    pub(crate) fn imports(&self) -> Vec<Import> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (name, library) in self.mapped.values() {
            if seen.insert((name.clone(), library.clone())) {
                out.push(Import {
                    name: name.clone(),
                    library: library.clone(),
                    code: true,
                });
            }
        }
        out
    }
}

#[derive(Default)]
pub(crate) struct Linker {
    pub(crate) code: Vec<u8>,
    pub(crate) data: Vec<u8>,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) relocations: Vec<Relocation>,
}

impl Linker {
    fn pad(buf: &mut Vec<u8>, align: usize, fill: u8) {
        while !buf.len().is_multiple_of(align) {
            buf.push(fill);
        }
    }

    pub(crate) fn populate(&mut self, module: &Module, catalog: &Catalog) {
        for asset in &module.assets {
            Self::pad(&mut self.data, 8, 0);
            let offset = self.data.len() as u64;
            let size = asset.bytes.len() as u64;
            self.data.extend_from_slice(&asset.bytes);
            if asset.bytes.is_empty() {
                self.data.push(0);
            }
            self.symbols.push(Symbol {
                name: asset.symbol.clone(),
                offset,
                size,
                code: false,
                exported: false,
            });
        }
        for table in &module.tables {
            Self::pad(&mut self.data, 8, 0);
            let offset = self.data.len() as u64;
            let size = (table.methods.len() * 8) as u64;
            for (idx, method) in table.methods.iter().enumerate() {
                let slot = offset + (idx * 8) as u64;
                let (target, _) = catalog.resolve(method);
                self.data.extend_from_slice(&[0; 8]);
                self.relocations.push(Relocation {
                    offset: slot,
                    symbol: target,
                    kind: Fixup::Pointer,
                    addend: 0,
                });
            }
            if table.methods.is_empty() {
                self.data.extend_from_slice(&[0; 8]);
            }
            self.symbols.push(Symbol {
                name: table.symbol.clone(),
                offset,
                size,
                code: false,
                exported: false,
            });
        }
    }

    pub(crate) fn append(
        &mut self,
        name: &str,
        exported: bool,
        body: (Vec<u8>, Vec<Relocation>),
    ) -> u64 {
        Self::pad(&mut self.code, 16, 0x90);
        let offset = self.code.len() as u64;
        let size = body.0.len() as u64;
        self.code.extend_from_slice(&body.0);
        for mut item in body.1 {
            item.offset += offset;
            self.relocations.push(item);
        }
        self.symbols.push(Symbol {
            name: name.to_owned(),
            offset,
            size,
            code: true,
            exported,
        });
        offset
    }

    pub(crate) fn functions(
        &mut self,
        flavor: Flavor,
        module: &Module,
        catalog: &Catalog,
    ) -> Result<(), Fault> {
        for func in &module.functions {
            let exported = func.linkage == Linkage::Exported;
            let body = translate(flavor, func, catalog)?;
            self.append(&func.symbol, exported, body);
        }
        Ok(())
    }
}

pub(crate) fn translate(
    flavor: Flavor,
    func: &Function,
    catalog: &Catalog,
) -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
    let frame = shape::build(flavor, func)?;
    Emitter::generate(func, &frame, catalog)
}

pub(crate) fn status(enc: &mut Encoder, dst: Register, output: &Type) {
    match output {
        Type::Void | Type::Aggregate { size: 0, .. } => enc.constant(dst, 0),
        Type::Integer { bits: 1, .. } => {
            enc.extend(dst, Register::Rax, Width::Byte);
            enc.constant(Register::R11, 1);
            enc.mask(dst, Register::R11);
        }
        Type::Integer {
            bits: b @ (8 | 16 | 32),
            signed: false,
        } => {
            enc.extend(dst, Register::Rax, Width::bits(*b));
        }
        Type::Integer {
            bits: b @ (8 | 16 | 32),
            signed: true,
        } => {
            enc.widen(dst, Register::Rax, Width::bits(*b));
        }
        _ => enc.copy(dst, Register::Rax),
    }
}

pub(crate) fn referenced(module: &Module, target: &str) -> bool {
    if module.externals.iter().any(|ext| ext.symbol == target) {
        return true;
    }
    module
        .functions
        .iter()
        .flat_map(|func| &func.blocks)
        .flat_map(|block| &block.body)
        .any(|inst| match &inst.op {
            Operation::Call { symbol, .. } | Operation::Symbol { name: symbol } => symbol == target,
            _ => false,
        })
}

pub(crate) fn signature(module: &Module, entry: &str) -> Result<Type, Fault> {
    if let Some(func) = module.functions.iter().find(|f| f.symbol == entry) {
        return Ok(func.output.clone());
    }
    if let Some(ext) = module.externals.iter().find(|e| e.symbol == entry) {
        return Ok(ext.output.clone());
    }
    Err(Fault::Unsupported(format!("unknown entry {entry}")))
}
