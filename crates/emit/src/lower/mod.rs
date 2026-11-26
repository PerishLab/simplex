pub mod abi;
mod call;
mod data;
mod expr;
pub mod fiber;
mod flow;
mod kind;
mod ops;

use self::abi::{Signature, Value};
use crate::Fault;
use crate::encode::Label;
use crate::machine::emit;
use crate::machine::linear::{Procedure, Slot, Step};
use crate::plan::Linker;
use simplex_elf::Plan;
use simplex_hir::{Expression, Routine, Statement, Type, Unit};
use simplex_runtime::{Hook, stubs};

#[derive(Clone)]
pub enum Storage {
    Stack(Slot),
    Heap { offset: i32, bit: u32 },
}

#[derive(Clone)]
pub struct Local {
    pub name: String,
    pub kind: Type,
    pub storage: Storage,
}

#[derive(Clone)]
pub struct Loop {
    pub head: Label,
    pub exit: Label,
    pub result: Option<(Slot, Type)>,
}

#[derive(Clone)]
pub struct Cleanup {
    pub name: String,
    pub symbol: String,
    pub offset: i32,
    pub bit: u32,
}

pub struct Async {
    pub cleanups: Vec<Cleanup>,
    pub output: Option<Slot>,
    pub exit: Label,
}

pub struct Resolved {
    pub symbol: String,
    pub inputs: Vec<Type>,
    pub output: Type,
    pub external: bool,
    pub foreign: bool,
}

pub struct Function<'a, 'b> {
    pub unit: &'a Unit,
    pub literals: &'b mut Vec<(String, Vec<u8>)>,
    pub proc: Procedure,
    pub scopes: Vec<Vec<Local>>,
    pub loops: Vec<Loop>,
    pub hidden: Option<u32>,
    pub output: Type,
    pub capture: Option<u32>,
    pub asyncs: Option<Async>,
}

impl<'a, 'b> Function<'a, 'b> {
    pub fn new(
        unit: &'a Unit,
        literals: &'b mut Vec<(String, Vec<u8>)>,
        proc: Procedure,
        output: Type,
    ) -> Self {
        Self {
            unit,
            literals,
            proc,
            scopes: vec![Vec::new()],
            loops: Vec::new(),
            hidden: None,
            output,
            capture: None,
            asyncs: None,
        }
    }

    pub fn sig(&self) -> Signature<'a> {
        Signature::new(&self.unit.catalog, false)
    }

    pub fn bind(&mut self, name: &str, kind: &Type, val: Value) {
        let heap = self
            .asyncs
            .as_ref()
            .and_then(|a| a.cleanups.iter().find(|c| c.name == name).cloned());
        if let (Some(clean), Some(env)) = (heap, self.capture) {
            let dst = Signature::displace(&mut self.proc, env, clean.offset);
            self.sig().store(&mut self.proc, dst, val, kind);
            self.toggle(clean.bit, true);
            if let Some(frame) = self.scopes.last_mut() {
                frame.push(Local {
                    name: name.to_owned(),
                    kind: kind.clone(),
                    storage: Storage::Heap {
                        offset: clean.offset,
                        bit: clean.bit,
                    },
                });
            }
            return;
        }
        let fp = self.sig().footprint(kind);
        let slot = self
            .proc
            .reserve(fp.size.max(8) as u32, fp.align.max(8) as u32);
        let ptr = Signature::base(&mut self.proc, slot, 0);
        self.sig().store(&mut self.proc, ptr, val, kind);
        if let Some(frame) = self.scopes.last_mut() {
            frame.push(Local {
                name: name.to_owned(),
                kind: kind.clone(),
                storage: Storage::Stack(slot),
            });
        }
    }

    pub fn lookup(&self, name: &str) -> Result<&Local, Fault> {
        for scope in self.scopes.iter().rev() {
            if let Some(local) = scope.iter().rev().find(|l| l.name == name) {
                return Ok(local);
            }
        }
        Err(Fault::Unimplemented(format!("unknown local {name}")))
    }

    pub fn address(&mut self, name: &str) -> Result<u32, Fault> {
        let storage = self.lookup(name)?.storage.clone();
        match storage {
            Storage::Stack(slot) => Ok(Signature::base(&mut self.proc, slot, 0)),
            Storage::Heap { offset, .. } => {
                let env = self.capture.unwrap_or(0);
                Ok(Signature::displace(&mut self.proc, env, offset))
            }
        }
    }

    pub fn scalar(&mut self, expr: &Expression) -> Result<u32, Fault> {
        match self.expression(expr)? {
            Value::Scalar(v) => Ok(v),
            other => {
                let kind = self.classify(expr);
                Ok(self.sig().materialize(&mut self.proc, other, &kind))
            }
        }
    }

    pub fn body(&mut self, stmts: &[Statement]) -> Result<(), Fault> {
        let last = stmts.len().saturating_sub(1);
        for (idx, stmt) in stmts.iter().enumerate() {
            if idx == last
                && self.output != Type::Void
                && let Statement::Evaluate(expr) = stmt
            {
                let val = self.expression(expr)?;
                self.complete(val);
                return Ok(());
            }
            self.statement(stmt)?;
        }
        if self.output == Type::Void {
            self.complete(Value::Void);
        }
        Ok(())
    }

    pub fn complete(&mut self, val: Value) {
        if let Some(state) = &self.asyncs {
            let exit = state.exit;
            if let Some(slot) = state.output {
                let dst = Signature::base(&mut self.proc, slot, 0);
                let out = self.output.clone();
                self.sig().store(&mut self.proc, dst, val, &out);
            }
            self.proc.emit(Step::Jump(exit));
            return;
        }
        let out = self.output.clone();
        self.sig().finish(&mut self.proc, self.hidden, val, &out);
    }

    pub fn resolve(&self, symbol: &str) -> Resolved {
        if let Some(r) = self.unit.routines.iter().find(|r| r.symbol == symbol) {
            let output = match r.asynchronous {
                true => Type::Fiber(Box::new(fiber::unwrap(&r.output))),
                false => r.output.clone(),
            };
            return Resolved {
                symbol: r.symbol.clone(),
                inputs: r.inputs.iter().map(|(_, t)| t.clone()).collect(),
                output,
                external: false,
                foreign: false,
            };
        }
        if let Some(imp) = self
            .unit
            .imports
            .iter()
            .find(|i| i.symbol == symbol || i.local == symbol)
        {
            let foreign =
                imp.library.as_deref() == Some("libc.so.6") || !imp.symbol.starts_with("sx$");
            return Resolved {
                symbol: imp.symbol.clone(),
                inputs: imp.inputs.clone(),
                output: imp.output.clone(),
                external: true,
                foreign,
            };
        }
        let runtime = symbol.starts_with("sx$rt$") || symbol.starts_with("sx$emit$");
        Resolved {
            symbol: symbol.to_owned(),
            inputs: Vec::new(),
            output: Type::Void,
            external: !runtime,
            foreign: false,
        }
    }
}

pub fn build(unit: &Unit) -> Result<Plan, Fault> {
    let mut linker = Linker::new();
    for asset in &unit.assets {
        linker.section(&asset.symbol, &asset.bytes);
    }
    for table in &unit.tables {
        linker.table(&table.symbol, &table.methods);
    }
    let mut literals = Vec::new();
    let mut procedures = Vec::new();
    let builder = fiber::Builder { unit };
    for routine in &unit.routines {
        if routine.asynchronous {
            let _ = &builder;
            return Err(Fault::Unimplemented(String::from("async")));
        } else {
            procedures.push(synchronous(unit, &mut literals, routine)?);
        }
    }
    for (name, bytes) in literals {
        linker.section(&name, &bytes);
    }
    let entry = match unit.shared {
        true => 0,
        false => {
            let Some(sym) = &unit.entry else {
                return Err(Fault::Entry);
            };
            let found = unit.routines.iter().find(|r| &r.symbol == sym);
            if found.is_none() && !unit.imports.iter().any(|i| &i.symbol == sym) {
                return Err(Fault::Entry);
            }
            let start = stubs().iter().any(|s| s.hook == Hook::Start);
            linker.startup(sym, found, start)?
        }
    };
    for proc in &procedures {
        let (bytes, relocs) = emit(proc)?;
        linker.function(&proc.name, &bytes, relocs);
    }
    linker.helpers()?;
    Ok(linker.assemble(unit, entry))
}

fn synchronous(
    unit: &Unit,
    literals: &mut Vec<(String, Vec<u8>)>,
    routine: &Routine,
) -> Result<Procedure, Fault> {
    let mut proc = Procedure::new(&routine.symbol);
    let sig = Signature::new(&unit.catalog, false);
    let entered = sig.enter(&mut proc, &routine.inputs, &routine.output);
    let mut func = Function::new(unit, literals, proc, routine.output.clone());
    func.hidden = entered.hidden;
    for ((name, kind), val) in routine.inputs.iter().zip(entered.values) {
        func.bind(name, kind, val);
    }
    func.body(&routine.body)?;
    Ok(func.proc)
}
