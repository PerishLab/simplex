mod builder;
mod clean;
mod compare;
mod control;
mod memory;
mod scalar;
mod types;
mod value;

use self::builder::Builder;
use self::clean::plan;
use self::types::{WIDE, convert};
use crate::tree::{Import, Routine, Unit};
use simplex_ir::{Asset, External, Function, Linkage, Module, Table};
use std::collections::BTreeMap;

pub const ALLOC: &str = "sx$rt$alloc";

pub struct Vault {
    pub assets: Vec<Asset>,
    pub allocated: bool,
    pub unresolved: BTreeMap<String, (Vec<simplex_ir::Type>, simplex_ir::Type)>,
}

impl Vault {
    pub fn intern(&mut self, symbol: &str) -> (String, usize) {
        if let Some(found) = self.assets.iter().find(|item| item.symbol == symbol) {
            return (found.symbol.clone(), found.bytes.len());
        }
        let index = self.assets.len();
        let name = format!("sx$ro$lit${index}");
        let bytes = symbol.as_bytes().to_vec();
        let count = bytes.len();
        self.assets.push(Asset {
            symbol: name.clone(),
            bytes,
        });
        (name, count)
    }
}

pub fn bridge(unit: &Unit) -> Module {
    let assets = unit
        .assets
        .iter()
        .map(|item| Asset {
            symbol: item.symbol.clone(),
            bytes: item.bytes.clone(),
        })
        .collect();
    let mut vault = Vault {
        assets,
        allocated: false,
        unresolved: BTreeMap::new(),
    };
    let tables = unit
        .tables
        .iter()
        .map(|item| Table {
            symbol: item.symbol.clone(),
            methods: item.methods.clone(),
        })
        .collect();
    let mut externals: Vec<External> = unit
        .imports
        .iter()
        .map(|item| external(item, unit))
        .collect();
    let mut functions = Vec::new();
    for routine in &unit.routines {
        let (built, cleaner) = lower(unit, &mut vault, routine);
        functions.push(built);
        if let Some(extra) = cleaner {
            functions.push(extra);
        }
    }
    if vault.allocated && !externals.iter().any(|item| item.symbol == ALLOC) {
        externals.push(External {
            symbol: String::from(ALLOC),
            linkage: Linkage::System,
            inputs: vec![WIDE],
            output: simplex_ir::Type::Pointer,
        });
    }
    for (symbol, (inputs, output)) in vault.unresolved {
        if !externals.iter().any(|item| item.symbol == symbol)
            && !functions.iter().any(|item| item.symbol == symbol)
        {
            externals.push(External {
                symbol,
                linkage: Linkage::System,
                inputs,
                output,
            });
        }
    }
    Module {
        shared: unit.shared,
        entry: unit.entry.clone(),
        assets: vault.assets,
        tables,
        externals,
        functions,
    }
}

fn external(item: &Import, unit: &Unit) -> External {
    let linkage = match item.library.as_deref() {
        None | Some("libc.so.6") => Linkage::System,
        Some(library) => Linkage::Dynamic(stem(library)),
    };
    let inputs = item
        .inputs
        .iter()
        .map(|kind| convert(kind, &unit.catalog))
        .filter(|kind| *kind != simplex_ir::Type::Void)
        .collect();
    let output = convert(&item.output, &unit.catalog);
    External {
        symbol: item.symbol.clone(),
        linkage,
        inputs,
        output,
    }
}

fn stem(library: &str) -> String {
    let clean = library
        .strip_suffix(".so")
        .or_else(|| library.strip_suffix(".dylib"))
        .or_else(|| library.strip_suffix(".dll"))
        .unwrap_or(library);
    clean.to_owned()
}

fn lower(unit: &Unit, vault: &mut Vault, routine: &Routine) -> (Function, Option<Function>) {
    let tracked = match routine.asynchronous {
        true => plan(&routine.inputs, &routine.body, &unit.catalog),
        false => None,
    };
    let cleaner = tracked
        .as_ref()
        .map(|spec| clean::emit::synthesize(&routine.symbol, spec, unit));
    let cleanup = cleaner.as_ref().map(|item| item.symbol.clone());
    let mut builder = Builder::new(unit, vault, routine, tracked);
    builder.populate(routine);
    let function = builder.finish(routine, cleanup);
    (function, cleaner)
}
