use crate::Fault;
use crate::intel::encode::Encoder;
use crate::intel::link::{Catalog, Linker, referenced, signature, status};
use crate::intel::{Condition, Flavor, Register};
use simplex_ir::{Linkage, Module};
use simplex_object::{Artifact, Format, Mode, Relocation};

const ALLOC: &str = "sx$rt$alloc";
const FREE: &str = "sx$rt$free";

pub fn compile(name: &str, module: &Module) -> Result<Artifact, Fault> {
    let mode = select(module);
    let catalog = catalog(module, mode);
    let mut linker = Linker::default();
    linker.populate(module, &catalog);
    let entry = match module.shared {
        true => 0,
        false => startup(&mut linker, module, &catalog)?,
    };
    linker.functions(Flavor::Linux, module, &catalog)?;
    if mode == Mode::Standalone {
        runtime(&mut linker, module)?;
    }
    if linker.code.is_empty() {
        let mut stub = Encoder::new();
        stub.leave();
        linker.append("sx$stub", false, stub.finish()?);
    }
    Ok(Artifact {
        format: Format::Elf,
        mode,
        name: name.to_owned(),
        entry,
        code: linker.code,
        data: linker.data,
        symbols: linker.symbols,
        imports: catalog.imports(),
        relocations: linker.relocations,
    })
}

fn select(module: &Module) -> Mode {
    if module.shared {
        return Mode::Shared;
    }
    let dynamic = module.externals.iter().any(|ext| match &ext.linkage {
        Linkage::Dynamic(_) => true,
        Linkage::System => ext.symbol != ALLOC && ext.symbol != FREE,
        _ => false,
    });
    match dynamic {
        true => Mode::Dynamic,
        false => Mode::Standalone,
    }
}

fn catalog(module: &Module, mode: Mode) -> Catalog {
    let mut out = Catalog::default();
    out.seed(module);
    if mode == Mode::Standalone {
        out.defined.insert(ALLOC.to_owned());
        out.defined.insert(FREE.to_owned());
        return out;
    }
    if referenced(module, ALLOC) {
        out.mapped.insert(
            ALLOC.to_owned(),
            ("malloc".to_owned(), "libc.so.6".to_owned()),
        );
    }
    if referenced(module, FREE) {
        out.mapped
            .insert(FREE.to_owned(), ("free".to_owned(), "libc.so.6".to_owned()));
    }
    for ext in &module.externals {
        if out.defined.contains(&ext.symbol) || out.mapped.contains_key(&ext.symbol) {
            continue;
        }
        let target = match ext.symbol.as_str() {
            ALLOC => "malloc".to_owned(),
            FREE => "free".to_owned(),
            other => other.to_owned(),
        };
        let lib = match &ext.linkage {
            Linkage::Dynamic(stem) => library(stem),
            _ => "libc.so.6".to_owned(),
        };
        out.mapped.insert(ext.symbol.clone(), (target, lib));
    }
    out
}

fn library(stem: &str) -> String {
    match stem.starts_with("lib") && stem.ends_with(".so") {
        true => stem.to_owned(),
        false => format!("lib{stem}.so"),
    }
}

fn startup(linker: &mut Linker, module: &Module, catalog: &Catalog) -> Result<u64, Fault> {
    let Some(entry) = &module.entry else {
        return Err(Fault::Unsupported("missing entry".to_owned()));
    };
    let output = signature(module, entry)?;
    let (target, _) = catalog.resolve(entry);
    let mut enc = Encoder::new();
    enc.constant(Register::Rbp, 0);
    enc.align();
    enc.call(&target);
    status(&mut enc, Register::Rdi, &output);
    enc.constant(Register::Rax, 231);
    enc.kernel();
    enc.trap();
    Ok(linker.append("_start", false, enc.finish()?))
}

fn runtime(linker: &mut Linker, module: &Module) -> Result<(), Fault> {
    if referenced(module, ALLOC) {
        linker.append(ALLOC, false, allocator()?);
    }
    if referenced(module, FREE) {
        linker.append(FREE, false, releaser()?);
    }
    Ok(())
}

fn allocator() -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
    let mut enc = Encoder::new();
    let ready = enc.fresh();
    enc.adjust(Register::Rdi, 4095);
    enc.bytes
        .extend_from_slice(&[0x48, 0x81, 0xE7, 0x00, 0xF0, 0xFF, 0xFF]);
    enc.test(Register::Rdi);
    enc.branch(Condition::Distinct, ready);
    enc.constant(Register::Rdi, 4096);
    enc.bind(ready);
    enc.copy(Register::Rsi, Register::Rdi);
    enc.constant(Register::Rdi, 0);
    enc.constant(Register::Rdx, 3);
    enc.constant(Register::R10, 0x22);
    enc.constant(Register::R8, -1);
    enc.constant(Register::R9, 0);
    enc.constant(Register::Rax, 9);
    enc.kernel();
    enc.leave();
    enc.finish()
}

fn releaser() -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
    let mut enc = Encoder::new();
    let unmap = enc.fresh();
    let done = enc.fresh();
    enc.test(Register::Rdi);
    enc.branch(Condition::Equal, done);
    enc.adjust(Register::Rsi, 4095);
    enc.bytes
        .extend_from_slice(&[0x48, 0x81, 0xE6, 0x00, 0xF0, 0xFF, 0xFF]);
    enc.test(Register::Rsi);
    enc.branch(Condition::Distinct, unmap);
    enc.constant(Register::Rsi, 4096);
    enc.bind(unmap);
    enc.constant(Register::Rax, 11);
    enc.kernel();
    enc.bind(done);
    enc.leave();
    enc.finish()
}
