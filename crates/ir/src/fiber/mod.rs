mod batch;
mod builder;
mod layout;
mod remap;
mod routine;
mod runtime;
mod step;
mod suspend;
mod sync;

use self::builder::{Builder, SIGNED, UNSIGNED};
use self::layout::RESULT;
use self::runtime::{DRIVE, DROP};
use crate::{External, Fault, Function, Linkage, Module, Operation, Terminator, Type};
use std::collections::HashSet;

pub const ALLOC: &str = "sx$rt$alloc";
pub const FREE: &str = "sx$rt$free";
const ENTRY: &str = "sx$entry$main";

pub fn lower(module: &mut Module) -> Result<(), Fault> {
    if !present(module) {
        return Ok(());
    }
    externals(module);
    adapt(module);
    let mut lowered = Vec::with_capacity(module.functions.len() * 2 + 12);
    let mut entry = None;
    for mut function in module.functions.drain(..) {
        if module.entry.as_deref() == Some(function.symbol.as_str()) && function.asynchronous {
            entry = Some((
                function.symbol.clone(),
                function.linkage.clone(),
                function.output.clone(),
            ));
        }
        if function.asynchronous {
            lowered.extend(routine::transform(&function));
        } else {
            sync::rewrite(&mut function);
            lowered.push(function);
        }
    }
    lowered.extend(runtime::synthesize());
    if let Some((symbol, linkage, output)) = entry {
        lowered.push(wrapper(&symbol, linkage, output));
        module.entry = Some(ENTRY.to_owned());
    }
    module.functions = lowered;
    Ok(())
}

fn present(module: &Module) -> bool {
    if module.functions.iter().any(|f| f.asynchronous) {
        return true;
    }
    module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.body)
        .any(|inst| {
            matches!(
                inst.op,
                Operation::Spawn { .. }
                    | Operation::Yield
                    | Operation::Await { .. }
                    | Operation::All { .. }
                    | Operation::Race { .. }
            )
        })
}

fn adapt(module: &mut Module) {
    let spawned: HashSet<String> = module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.body)
        .filter_map(|inst| match &inst.op {
            Operation::Spawn { symbol, .. } => Some(symbol.clone()),
            _ => None,
        })
        .collect();
    for ext in &mut module.externals {
        if spawned.contains(&ext.symbol) {
            ext.output = Type::Pointer;
        }
    }
}

fn externals(module: &mut Module) {
    if !module.externals.iter().any(|e| e.symbol == ALLOC) {
        module.externals.push(External {
            symbol: ALLOC.to_owned(),
            linkage: Linkage::System,
            inputs: vec![UNSIGNED],
            output: Type::Pointer,
        });
    }
    if !module.externals.iter().any(|e| e.symbol == FREE) {
        module.externals.push(External {
            symbol: FREE.to_owned(),
            linkage: Linkage::System,
            inputs: vec![Type::Pointer, UNSIGNED],
            output: Type::Void,
        });
    }
}

fn wrapper(target: &str, linkage: Linkage, output: Type) -> Function {
    let ret = match output == Type::Void {
        true => SIGNED,
        false => output.clone(),
    };
    let mut b = Builder::new(ENTRY.to_owned(), linkage, ret);
    let entry = b.block();
    b.select(entry);
    let fiber = b.call(target, Vec::new(), Vec::new(), Type::Pointer);
    let _ = b.call(DRIVE, vec![fiber], vec![Type::Pointer], Type::Void);
    let val = match output == Type::Void {
        true => b.signed(0),
        false => {
            let rptr = b.offset(fiber, RESULT);
            b.load(rptr, output)
        }
    };
    let _ = b.call(DROP, vec![fiber], vec![Type::Pointer], Type::Void);
    b.blocks[entry as usize].exit = Terminator::Return(Some(val));
    b.finish()
}
