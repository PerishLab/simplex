use crate::Fault;
use crate::intel::encode::Encoder;
use crate::intel::link::{Catalog, Linker, referenced, signature, status};
use crate::intel::{Flavor, Register};
use simplex_ir::{Linkage, Module};
use simplex_object::{Artifact, Format, Mode};

const ALLOC: &str = "sx$rt$alloc";
const FREE: &str = "sx$rt$free";
const EXIT: &str = "ExitProcess";
const MODE: &str = "_setmode";

pub fn compile(name: &str, module: &Module) -> Result<Artifact, Fault> {
    let mode = match module.shared {
        true => Mode::Shared,
        false => Mode::Dynamic,
    };
    let catalog = catalog(module);
    let mut linker = Linker::default();
    linker.populate(module, &catalog);
    let entry = match module.shared {
        true => 0,
        false => startup(&mut linker, module, &catalog)?,
    };
    linker.functions(Flavor::Windows, module, &catalog)?;
    if linker.code.is_empty() {
        let mut stub = Encoder::new();
        stub.constant(Register::Rax, 1);
        stub.leave();
        linker.append("sx$stub", false, stub.finish()?);
    }
    Ok(Artifact {
        format: Format::Portable,
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

fn catalog(module: &Module) -> Catalog {
    let mut out = Catalog::default();
    out.seed(module);
    if !module.shared {
        out.mapped.insert(
            EXIT.to_owned(),
            (EXIT.to_owned(), "KERNEL32.dll".to_owned()),
        );
    }
    if referenced(module, ALLOC) {
        out.mapped.insert(
            ALLOC.to_owned(),
            ("malloc".to_owned(), "ucrtbase.dll".to_owned()),
        );
    }
    if referenced(module, FREE) {
        out.mapped.insert(
            FREE.to_owned(),
            ("free".to_owned(), "ucrtbase.dll".to_owned()),
        );
    }
    for ext in &module.externals {
        if out.defined.contains(&ext.symbol) || out.mapped.contains_key(&ext.symbol) {
            continue;
        }
        let (target, lib) = match &ext.linkage {
            Linkage::Dynamic(stem) => (ext.symbol.clone(), library(stem)),
            _ => (system(&ext.symbol), "ucrtbase.dll".to_owned()),
        };
        out.mapped.insert(ext.symbol.clone(), (target, lib));
    }
    if !module.shared && binary(&out) {
        out.mapped.insert(
            MODE.to_owned(),
            (MODE.to_owned(), "ucrtbase.dll".to_owned()),
        );
    }
    out
}

fn binary(catalog: &Catalog) -> bool {
    catalog
        .mapped
        .values()
        .any(|(name, lib)| lib == "ucrtbase.dll" && matches!(name.as_str(), "_write" | "_read"))
}

fn system(symbol: &str) -> String {
    match symbol {
        ALLOC => "malloc".to_owned(),
        FREE => "free".to_owned(),
        "write" | "read" | "close" | "open" | "lseek" | "unlink" | "isatty" | "fileno" | "dup"
        | "dup2" => format!("_{symbol}"),
        other => other.to_owned(),
    }
}

fn library(stem: &str) -> String {
    match stem.ends_with(".dll") {
        true => stem.to_owned(),
        false => format!("{stem}.dll"),
    }
}

fn startup(linker: &mut Linker, module: &Module, catalog: &Catalog) -> Result<u64, Fault> {
    let Some(entry) = &module.entry else {
        return Err(Fault::Unsupported("missing entry".to_owned()));
    };
    let output = signature(module, entry)?;
    let (target, _) = catalog.resolve(entry);
    let mut enc = Encoder::new();
    enc.adjust(Register::Rsp, -40);
    if catalog.mapped.contains_key(MODE) {
        for handle in [1i64, 2] {
            enc.constant(Register::Rcx, handle);
            enc.constant(Register::Rdx, 0x8000);
            enc.call(MODE);
        }
    }
    enc.call(&target);
    status(&mut enc, Register::Rcx, &output);
    enc.call(EXIT);
    enc.trap();
    Ok(linker.append("_start", false, enc.finish()?))
}
