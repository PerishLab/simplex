mod call;
mod dominance;
mod operation;

use self::dominance::Tree;
use crate::{Block, Fault, Function, Module, Type};
use std::collections::{HashMap, HashSet};

const LIMIT: u32 = 16_777_216;

pub struct Callable {
    pub asynchronous: bool,
    pub inputs: Vec<Type>,
    pub output: Type,
}

pub struct Catalog {
    pub callables: HashMap<String, Callable>,
}

#[derive(Clone, Copy)]
pub struct Spot {
    pub block: usize,
    pub order: u32,
}

#[derive(Clone)]
struct Origin {
    kind: Type,
    block: Option<usize>,
    order: u32,
}

pub struct Context<'a> {
    pub catalog: &'a Catalog,
    pub function: &'a Function,
    pub(super) tree: Tree,
    defs: Vec<Option<Origin>>,
}

pub fn check(module: &Module) -> Result<(), Fault> {
    let catalog = globals(module)?;
    if let Some(entry) = &module.entry
        && !module.functions.iter().any(|item| &item.symbol == entry)
    {
        return Err(Fault::Invalid(format!("missing entry function {entry}")));
    }
    for function in &module.functions {
        routine(&catalog, function)?;
    }
    Ok(())
}

fn globals(module: &Module) -> Result<Catalog, Fault> {
    let mut names = HashSet::new();
    let mut callables = HashMap::new();
    for asset in &module.assets {
        register(&mut names, &asset.symbol, "asset")?;
    }
    for table in &module.tables {
        register(&mut names, &table.symbol, "table")?;
        if table.methods.iter().any(String::is_empty) {
            return Err(Fault::Invalid(format!(
                "empty method in table {}",
                table.symbol
            )));
        }
    }
    for external in &module.externals {
        register(&mut names, &external.symbol, "external")?;
        validate(&external.symbol, &external.inputs, &external.output)?;
        callables.insert(
            external.symbol.clone(),
            Callable {
                asynchronous: false,
                inputs: external.inputs.clone(),
                output: external.output.clone(),
            },
        );
    }
    for function in &module.functions {
        register(&mut names, &function.symbol, "function")?;
        let inputs: Vec<Type> = function.inputs.iter().map(|(_, k)| k.clone()).collect();
        validate(&function.symbol, &inputs, &function.output)?;
        callables.insert(
            function.symbol.clone(),
            Callable {
                asynchronous: function.asynchronous,
                inputs,
                output: function.output.clone(),
            },
        );
    }
    Ok(Catalog { callables })
}

fn register<'a>(names: &mut HashSet<&'a str>, symbol: &'a str, label: &str) -> Result<(), Fault> {
    if symbol.is_empty() {
        return Err(Fault::Invalid(format!("empty {label} symbol")));
    }
    match names.insert(symbol) {
        true => Ok(()),
        false => Err(Fault::Invalid(format!("duplicate symbol {symbol}"))),
    }
}

fn validate(symbol: &str, inputs: &[Type], output: &Type) -> Result<(), Fault> {
    if !output.valid() {
        return Err(Fault::Invalid(format!("{symbol}: invalid output type")));
    }
    for kind in inputs {
        if !kind.concrete() {
            return Err(Fault::Invalid(format!("{symbol}: invalid input type")));
        }
    }
    Ok(())
}

fn routine(catalog: &Catalog, function: &Function) -> Result<(), Fault> {
    if function.blocks.is_empty() {
        return Err(Fault::Invalid(format!(
            "{}: empty block list",
            function.symbol
        )));
    }
    if !function.blocks[0].parameters.is_empty() {
        return Err(Fault::Invalid(format!(
            "{}: entry block has parameters",
            function.symbol
        )));
    }
    if function.values > LIMIT {
        return Err(Fault::Invalid(format!(
            "{}: value count exceeds limit",
            function.symbol
        )));
    }
    cleanup(catalog, function)?;
    let map = index(&function.symbol, &function.blocks)?;
    let tree = Tree::build(&function.blocks, map);
    let mut ctx = Context {
        catalog,
        function,
        tree,
        defs: vec![None; function.values as usize],
    };
    for (idx, (id, kind)) in function.inputs.iter().enumerate() {
        ctx.define(*id, kind.clone(), None, idx as u32)?;
    }
    ctx.record()?;
    ctx.inspect()
}

fn cleanup(catalog: &Catalog, function: &Function) -> Result<(), Fault> {
    let Some(name) = &function.cleanup else {
        return Ok(());
    };
    let Some(entry) = catalog.callables.get(name) else {
        return Err(Fault::Invalid(format!(
            "{}: unknown cleanup {name}",
            function.symbol
        )));
    };
    let valid = !entry.asynchronous
        && entry.inputs.as_slice() == [Type::Pointer]
        && entry.output == Type::Void;
    match valid {
        true => Ok(()),
        false => Err(Fault::Invalid(format!(
            "{}: invalid cleanup signature {name}",
            function.symbol
        ))),
    }
}

fn index(symbol: &str, blocks: &[Block]) -> Result<HashMap<u32, usize>, Fault> {
    let mut map = HashMap::with_capacity(blocks.len());
    for (pos, block) in blocks.iter().enumerate() {
        if map.insert(block.id, pos).is_some() {
            return Err(Fault::Invalid(format!(
                "{symbol}: duplicate block {}",
                block.id
            )));
        }
    }
    Ok(map)
}

impl Context<'_> {
    fn record(&mut self) -> Result<(), Fault> {
        for (pos, block) in self.function.blocks.iter().enumerate() {
            for (id, kind) in &block.parameters {
                self.define(*id, kind.clone(), Some(pos), 0)?;
            }
            for (idx, inst) in block.body.iter().enumerate() {
                if !inst.kind.valid() {
                    return Err(self.fail("invalid instruction result type"));
                }
                if inst.kind != Type::Void {
                    self.define(inst.value, inst.kind.clone(), Some(pos), (idx as u32) + 1)?;
                }
            }
        }
        Ok(())
    }

    fn inspect(&self) -> Result<(), Fault> {
        for (pos, block) in self.function.blocks.iter().enumerate() {
            for (idx, inst) in block.body.iter().enumerate() {
                let spot = Spot {
                    block: pos,
                    order: (idx as u32) + 1,
                };
                self.instruction(spot, inst)?;
            }
            let end = Spot {
                block: pos,
                order: (block.body.len() as u32) + 1,
            };
            self.terminator(end, &block.exit)?;
        }
        Ok(())
    }

    pub fn fail(&self, message: &str) -> Fault {
        Fault::Invalid(format!("{}: {message}", self.function.symbol))
    }

    pub fn define(
        &mut self,
        id: u32,
        kind: Type,
        block: Option<usize>,
        order: u32,
    ) -> Result<(), Fault> {
        if !kind.concrete() {
            return Err(self.fail(&format!("invalid value type for %{id}")));
        }
        let Some(slot) = self.defs.get_mut(id as usize) else {
            return Err(self.fail(&format!("value %{id} exceeds declared values")));
        };
        if slot.is_some() {
            return Err(self.fail(&format!("duplicate definition of %{id}")));
        }
        *slot = Some(Origin { kind, block, order });
        Ok(())
    }

    pub fn lookup(&self, spot: Spot, id: u32) -> Result<&Type, Fault> {
        let Some(Some(origin)) = self.defs.get(id as usize) else {
            return Err(self.fail(&format!("undefined value %{id}")));
        };
        if origin.block == Some(spot.block) && origin.order >= spot.order {
            return Err(self.fail(&format!("value %{id} used before definition")));
        }
        if !self.tree.dominates(origin.block, spot.block) {
            return Err(self.fail(&format!("value %{id} does not dominate use")));
        }
        Ok(&origin.kind)
    }

    pub fn require(&self, spot: Spot, id: u32, expected: &Type) -> Result<(), Fault> {
        let actual = self.lookup(spot, id)?;
        match actual == expected {
            true => Ok(()),
            false => Err(self.fail(&format!("type mismatch on %{id}"))),
        }
    }
}
