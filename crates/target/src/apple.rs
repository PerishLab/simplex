use crate::Fault;
use crate::arm::{Encoder, Resolver, generate};
use simplex_ir::{Function, Linkage, Module, Type};
use simplex_object::{Artifact, Fixup, Format, Import, Mode, Relocation, Symbol};
use std::collections::BTreeSet;

struct Directory {
    defined: BTreeSet<String>,
    externals: BTreeSet<String>,
}

impl Resolver for Directory {
    fn symbol(&self, name: &str) -> (String, bool) {
        if self.defined.contains(name) {
            return (format!("_{name}"), false);
        }
        match name {
            "sx$rt$alloc" => (String::from("_malloc"), true),
            "sx$rt$free" => (String::from("_free"), true),
            other => {
                let imported = self.externals.contains(other) || !self.defined.contains(other);
                (format!("_{other}"), imported)
            }
        }
    }
}

struct Linker<'a> {
    module: &'a Module,
    directory: Directory,
    code: Vec<u8>,
    data: Vec<u8>,
    symbols: Vec<Symbol>,
    imported: Vec<Import>,
    relocations: Vec<Relocation>,
}

pub fn compile(name: &str, module: &Module) -> Result<Artifact, Fault> {
    let mut defined = BTreeSet::new();
    for item in &module.functions {
        defined.insert(item.symbol.clone());
    }
    for asset in &module.assets {
        defined.insert(asset.symbol.clone());
    }
    for table in &module.tables {
        defined.insert(table.symbol.clone());
    }
    let mut externals = BTreeSet::new();
    for ext in &module.externals {
        externals.insert(ext.symbol.clone());
    }
    let linker = Linker {
        module,
        directory: Directory { defined, externals },
        code: Vec::new(),
        data: Vec::new(),
        symbols: Vec::new(),
        imported: Vec::new(),
        relocations: Vec::new(),
    };
    linker.build(name)
}

impl Linker<'_> {
    fn build(mut self, name: &str) -> Result<Artifact, Fault> {
        self.imports();
        self.assets();
        self.tables();
        self.functions()?;
        let entry = self.startup()?;
        if self.code.is_empty() {
            let mut fallback = Encoder::new();
            fallback.leave();
            self.code.extend_from_slice(&fallback.bytes);
        }
        let mode = match self.module.shared {
            true => Mode::Shared,
            false => Mode::Dynamic,
        };
        Ok(Artifact {
            format: Format::Mach,
            mode,
            name: name.to_owned(),
            entry,
            code: self.code,
            data: self.data,
            symbols: self.symbols,
            imports: self.imported,
            relocations: self.relocations,
        })
    }

    fn imports(&mut self) {
        let mut seen = BTreeSet::new();
        for ext in &self.module.externals {
            if self.directory.defined.contains(&ext.symbol) {
                continue;
            }
            let (mapped, _) = self.directory.symbol(&ext.symbol);
            if !seen.insert(mapped.clone()) {
                continue;
            }
            let library = match &ext.linkage {
                Linkage::Dynamic(stem) => format!("@executable_path/{stem}.dylib"),
                _ => String::from("/usr/lib/libSystem.B.dylib"),
            };
            let code = !ext.symbol.starts_with("sx$vt$");
            self.imported.push(Import {
                name: mapped,
                library,
                code,
            });
        }
    }

    fn assets(&mut self) {
        for asset in &self.module.assets {
            pad(&mut self.data, 8);
            let offset = self.data.len() as u64;
            let size = asset.bytes.len() as u64;
            self.data.extend_from_slice(&asset.bytes);
            if asset.bytes.is_empty() {
                self.data.push(0);
            }
            self.symbols.push(Symbol {
                name: format!("_{}", asset.symbol),
                offset,
                size,
                code: false,
                exported: false,
            });
        }
    }

    fn tables(&mut self) {
        for table in &self.module.tables {
            pad(&mut self.data, 8);
            let offset = self.data.len() as u64;
            let size = (table.methods.len() * 8) as u64;
            for (index, method) in table.methods.iter().enumerate() {
                let slot = offset + (index * 8) as u64;
                self.data.extend_from_slice(&[0u8; 8]);
                let (mapped, _) = self.directory.symbol(method);
                self.relocations.push(Relocation {
                    offset: slot,
                    symbol: mapped,
                    kind: Fixup::Pointer,
                    addend: 0,
                });
            }
            if table.methods.is_empty() {
                self.data.extend_from_slice(&[0u8; 8]);
            }
            self.symbols.push(Symbol {
                name: format!("_{}", table.symbol),
                offset,
                size,
                code: false,
                exported: self.module.shared,
            });
        }
    }

    fn functions(&mut self) -> Result<(), Fault> {
        for item in &self.module.functions {
            pad(&mut self.code, 4);
            let offset = self.code.len() as u64;
            let (bytes, patches) = generate(item, &self.directory)?;
            let size = bytes.len() as u64;
            self.code.extend_from_slice(&bytes);
            for mut patch in patches {
                patch.offset += offset;
                register(&patch.symbol, &self.symbols, &mut self.imported);
                self.relocations.push(patch);
            }
            self.symbols.push(Symbol {
                name: format!("_{}", item.symbol),
                offset,
                size,
                code: true,
                exported: matches!(item.linkage, Linkage::Exported),
            });
        }
        Ok(())
    }

    fn startup(&mut self) -> Result<u64, Fault> {
        if self.module.shared {
            return Ok(0);
        }
        let Some(target) = &self.module.entry else {
            return Ok(0);
        };
        pad(&mut self.code, 4);
        let offset = self.code.len() as u64;
        let mut encoder = Encoder::new();
        encoder.enter();
        let (mapped, _) = self.directory.symbol(target);
        encoder.call(&mapped);
        let routine: Option<&Function> = self.module.functions.iter().find(|f| f.symbol == *target);
        if routine.is_some_and(|f| f.output == Type::Void) {
            encoder.constant(0, 0);
        }
        encoder.exit();
        let (bytes, patches) = encoder.finish()?;
        let size = bytes.len() as u64;
        self.code.extend_from_slice(&bytes);
        for mut patch in patches {
            patch.offset += offset;
            register(&patch.symbol, &self.symbols, &mut self.imported);
            self.relocations.push(patch);
        }
        let wrapper = match self.symbols.iter().any(|s| s.name == "_main") {
            true => String::from("_start"),
            false => String::from("_main"),
        };
        self.symbols.push(Symbol {
            name: wrapper,
            offset,
            size,
            code: true,
            exported: true,
        });
        Ok(offset)
    }
}

fn register(symbol: &str, symbols: &[Symbol], imported: &mut Vec<Import>) {
    if symbols.iter().any(|s| s.name == symbol) || imported.iter().any(|i| i.name == symbol) {
        return;
    }
    if let Some(rest) = symbol.strip_prefix("_sx$vt$") {
        let parts: Vec<&str> = rest.split('$').collect();
        if parts.len() >= 2 {
            let stem = parts[1];
            imported.push(Import {
                name: symbol.to_owned(),
                library: format!("@executable_path/{stem}.dylib"),
                code: false,
            });
            return;
        }
    }
    if symbol == "_malloc" || symbol == "_free" {
        imported.push(Import {
            name: symbol.to_owned(),
            library: String::from("/usr/lib/libSystem.B.dylib"),
            code: true,
        });
    }
}

fn pad(buffer: &mut Vec<u8>, step: usize) {
    while !buffer.len().is_multiple_of(step) {
        buffer.push(0);
    }
}
