use super::Vault;
use super::clean::Plan;
use super::types::{WIDE, convert, infer};
use crate::tree::{Expression, Routine, Statement, Unit};
use simplex_ir::{Block, Function, Instruction, Linkage, Operation, Terminator};

#[derive(Clone)]
pub struct Local {
    pub name: String,
    pub kind: crate::Type,
    pub address: u32,
    pub bit: Option<u32>,
}

#[derive(Clone)]
pub struct Loop {
    pub head: u32,
    pub exit: u32,
    pub kind: crate::Type,
    pub broke: bool,
}

pub struct Resolved {
    pub symbol: String,
    pub inputs: Vec<crate::Type>,
    pub output: crate::Type,
    pub payload: crate::Type,
    pub asynchronous: bool,
    pub missing: bool,
}

pub struct Builder<'a, 'b> {
    pub unit: &'a Unit,
    pub vault: &'b mut Vault,
    pub inputs: Vec<(u32, simplex_ir::Type)>,
    pub output: crate::Type,
    pub values: u32,
    pub blocks: Vec<Block>,
    pub current: usize,
    pub sealed: bool,
    pub slots: Vec<Instruction>,
    pub scopes: Vec<Vec<Local>>,
    pub loops: Vec<Loop>,
    pub plan: Option<Plan>,
    pub environment: Option<u32>,
    pub order: usize,
}

impl<'a, 'b> Builder<'a, 'b> {
    pub fn new(
        unit: &'a Unit,
        vault: &'b mut Vault,
        routine: &Routine,
        plan: Option<Plan>,
    ) -> Self {
        let entry = Block {
            id: 0,
            parameters: Vec::new(),
            body: Vec::new(),
            exit: Terminator::Trap { code: 0 },
        };
        Self {
            unit,
            vault,
            inputs: Vec::new(),
            output: routine.output.clone(),
            values: 0,
            blocks: vec![entry],
            current: 0,
            sealed: false,
            slots: Vec::new(),
            scopes: vec![Vec::new()],
            loops: Vec::new(),
            plan,
            environment: None,
            order: 0,
        }
    }

    pub fn fresh(&mut self) -> u32 {
        let id = self.values;
        self.values += 1;
        id
    }

    pub fn emit(&mut self, kind: simplex_ir::Type, op: Operation) -> u32 {
        let value = self.fresh();
        if !self.sealed {
            self.blocks[self.current]
                .body
                .push(Instruction { value, kind, op });
        }
        value
    }

    pub fn slot(&mut self, size: u64, align: u64) -> u32 {
        let value = self.fresh();
        self.slots.push(Instruction {
            value,
            kind: simplex_ir::Type::Pointer,
            op: Operation::Slot {
                size: size.max(1) as u32,
                align: align.max(1) as u32,
            },
        });
        value
    }

    pub fn block(&mut self, parameters: Vec<(u32, simplex_ir::Type)>) -> u32 {
        let id = self.blocks.len() as u32;
        self.blocks.push(Block {
            id,
            parameters,
            body: Vec::new(),
            exit: Terminator::Trap { code: 0 },
        });
        id
    }

    pub fn activate(&mut self, target: u32, active: bool) {
        self.current = target as usize;
        self.sealed = !active;
        if !active {
            self.blocks[self.current].exit = Terminator::Trap { code: 0 };
        }
    }

    pub fn terminate(&mut self, exit: Terminator) {
        if !self.sealed {
            self.blocks[self.current].exit = exit;
            self.sealed = true;
        }
    }

    pub fn populate(&mut self, routine: &Routine) {
        let catalog = &self.unit.catalog;
        let mut incoming = Vec::with_capacity(routine.inputs.len());
        for (name, kind) in &routine.inputs {
            let lowered = convert(kind, catalog);
            let value = match lowered == simplex_ir::Type::Void {
                true => 0,
                false => {
                    let id = self.fresh();
                    self.inputs.push((id, lowered));
                    id
                }
            };
            incoming.push((name.clone(), kind.clone(), value));
        }
        self.setup();
        for (name, kind, value) in incoming {
            self.bind(&name, &kind, value);
        }
        self.statements(&routine.body, true);
        if !self.sealed {
            let fallback = self.zero(&routine.output);
            self.complete(fallback);
        }
    }

    fn setup(&mut self) {
        let Some(spec) = &self.plan else {
            return;
        };
        let address = self.emit(
            simplex_ir::Type::Pointer,
            Operation::Slot {
                size: spec.size,
                align: 8,
            },
        );
        let zero = self.integer(0, WIDE);
        let _ = self.emit(
            simplex_ir::Type::Void,
            Operation::Store {
                address,
                value: zero,
                kind: WIDE,
                volatile: false,
            },
        );
        self.environment = Some(address);
    }

    pub fn statements(&mut self, items: &[Statement], root: bool) {
        let last = items.len().saturating_sub(1);
        for (index, item) in items.iter().enumerate() {
            if self.sealed {
                break;
            }
            let tail = root && index == last && self.output != crate::Type::Void;
            if tail && let Statement::Evaluate(expr) = item {
                let actual = self.classify(expr);
                let raw = self.expression(expr);
                let expected = self.output.clone();
                let value = self.recast(raw, &actual, &expected);
                self.complete(value);
                return;
            }
            self.statement(item);
        }
    }

    pub fn complete(&mut self, value: u32) {
        let lowered = convert(&self.output, &self.unit.catalog);
        match lowered {
            simplex_ir::Type::Void => self.terminate(Terminator::Return(None)),
            _ => self.terminate(Terminator::Return(Some(value))),
        }
    }

    pub fn classify(&self, expr: &Expression) -> crate::Type {
        match expr {
            Expression::Call { symbol, kind, .. } if *kind == crate::Type::Void => {
                self.resolve(symbol).output
            }
            other => infer(other),
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Local> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|item| item.name == name)
    }

    pub fn finish(mut self, routine: &Routine, cleanup: Option<String>) -> Function {
        let anchor = usize::from(self.environment.is_some());
        if !self.slots.is_empty() {
            let tail = self.blocks[0].body.split_off(anchor);
            self.blocks[0].body.extend(self.slots);
            self.blocks[0].body.extend(tail);
        }
        let linkage = match routine.exported {
            true => Linkage::Exported,
            false => Linkage::Internal,
        };
        let output = convert(&routine.output, &self.unit.catalog);
        Function {
            symbol: routine.symbol.clone(),
            linkage,
            asynchronous: routine.asynchronous,
            cleanup,
            inputs: self.inputs,
            output,
            values: self.values,
            blocks: self.blocks,
        }
    }
}
