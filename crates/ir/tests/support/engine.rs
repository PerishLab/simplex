use super::Value;
use super::math::{binary, cast, mask, unary};
use simplex_ir::{Function, Module, Operation, Target, Terminator, Type};
use std::collections::HashMap;

const BASE: u64 = 0x1000_0000;
const FUNC: u64 = 0x5000_0000;

pub struct Machine<'a> {
    pub module: &'a Module,
    pub functions: HashMap<String, usize>,
    pub symbols: HashMap<String, u64>,
    pub names: HashMap<u64, String>,
    pub blocks: HashMap<u64, Vec<u8>>,
    pub stdout: Vec<u8>,
    pub cursor: u64,
}

impl<'a> Machine<'a> {
    pub fn new(module: &'a Module) -> Self {
        let mut functions = HashMap::new();
        let mut symbols = HashMap::new();
        let mut names = HashMap::new();
        for (idx, func) in module.functions.iter().enumerate() {
            let addr = FUNC + ((idx as u64 + 1) * 16);
            functions.insert(func.symbol.clone(), idx);
            symbols.insert(func.symbol.clone(), addr);
            names.insert(addr, func.symbol.clone());
        }
        Self {
            module,
            functions,
            symbols,
            names,
            blocks: HashMap::new(),
            stdout: Vec::new(),
            cursor: BASE,
        }
    }

    pub fn alloc(&mut self, size: u64) -> u64 {
        let addr = self.cursor;
        let span = size.max(8) as usize;
        self.cursor += (span as u64 + 63) & !63;
        self.blocks.insert(addr, vec![0xCD; span]);
        addr
    }

    pub fn free(&mut self, addr: u64, size: u64) {
        let removed = self.blocks.remove(&addr).expect("double or invalid free");
        assert_eq!(removed.len(), size.max(8) as usize, "free size mismatch");
    }

    pub fn read(&self, addr: u64, len: usize) -> Vec<u8> {
        let (&base, buf) = self
            .blocks
            .iter()
            .find(|&(&start, data)| addr >= start && (addr - start) as usize + len <= data.len())
            .expect("invalid memory read");
        let off = (addr - base) as usize;
        buf[off..off + len].to_vec()
    }

    pub fn write(&mut self, addr: u64, bytes: &[u8]) {
        let (&base, buf) = self
            .blocks
            .iter_mut()
            .find(|(start, data)| {
                addr >= **start && (addr - **start) as usize + bytes.len() <= data.len()
            })
            .expect("invalid memory write");
        let off = (addr - base) as usize;
        buf[off..off + bytes.len()].copy_from_slice(bytes);
    }

    pub fn invoke(&mut self, name: &str, args: Vec<Value>) -> Value {
        match name {
            "sx$rt$alloc" => Value::Word(self.alloc(args[0].word())),
            "sx$rt$free" => {
                self.free(args[0].word(), args[1].word());
                Value::Void
            }
            "emit" => {
                self.stdout.push(args[0].word() as u8);
                Value::Void
            }
            other => {
                let idx = self.functions[other];
                let func = &self.module.functions[idx];
                self.execute(func, args)
            }
        }
    }

    fn execute(&mut self, func: &Function, args: Vec<Value>) -> Value {
        let mut regs: HashMap<u32, Value> = HashMap::new();
        let mut owned: Vec<u64> = Vec::new();
        for ((id, _), val) in func.inputs.iter().zip(args) {
            regs.insert(*id, val);
        }
        let map: HashMap<u32, usize> = func
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id, i))
            .collect();
        let mut cur = 0usize;
        loop {
            let blk = &func.blocks[cur];
            self.run(&blk.body, &mut regs, &mut owned);
            match self.terminate(&blk.exit, (&map, func), &mut regs) {
                Step::Jump(next) => cur = next,
                Step::Return(out) => {
                    release(&mut self.blocks, &owned);
                    return out;
                }
            }
        }
    }

    fn run(
        &mut self,
        body: &[simplex_ir::Instruction],
        regs: &mut HashMap<u32, Value>,
        owned: &mut Vec<u64>,
    ) {
        for inst in body {
            let val = self.step(&inst.op, &inst.kind, regs, owned);
            if inst.kind != Type::Void {
                regs.insert(inst.value, val);
            }
        }
    }

    fn terminate(
        &self,
        exit: &Terminator,
        ctx: (&HashMap<u32, usize>, &Function),
        regs: &mut HashMap<u32, Value>,
    ) -> Step {
        match exit {
            Terminator::Jump(edge) => Step::Jump(jump(ctx.0, ctx.1, edge, regs)),
            Terminator::Branch {
                condition,
                truthy,
                falsy,
            } => {
                let chosen = match regs[condition].word() != 0 {
                    true => truthy,
                    false => falsy,
                };
                Step::Jump(jump(ctx.0, ctx.1, chosen, regs))
            }
            Terminator::Switch {
                value,
                cases,
                default,
            } => {
                let key = regs[value].word();
                let chosen = cases
                    .iter()
                    .find(|(tag, _)| *tag == key)
                    .map_or(default, |(_, edge)| edge);
                Step::Jump(jump(ctx.0, ctx.1, chosen, regs))
            }
            Terminator::Return(opt) => {
                Step::Return(opt.map_or(Value::Void, |id| regs[&id].clone()))
            }
            Terminator::Trap { code } => panic!("trap {code}"),
        }
    }

    fn step(
        &mut self,
        op: &Operation,
        out: &Type,
        regs: &HashMap<u32, Value>,
        owned: &mut Vec<u64>,
    ) -> Value {
        match op {
            Operation::Integer { value, kind } => Value::Word(mask(*value, kind)),
            Operation::Symbol { name } => Value::Word(self.symbols[name]),
            Operation::Slot { size, .. } => {
                let addr = self.alloc(u64::from((*size).max(8)));
                owned.push(addr);
                Value::Word(addr)
            }
            Operation::Unary { op: u, value, kind } => unary(*u, regs[value].word(), kind),
            Operation::Binary {
                op: b,
                left,
                right,
                kind,
            } => binary(*b, (regs[left].word(), regs[right].word()), kind),
            Operation::Cast {
                op: c,
                value,
                from,
                kind,
            } => cast(*c, regs[value].word(), from, kind),
            Operation::Pair { low, high } => Value::Pair(regs[low].word(), regs[high].word()),
            Operation::Low { pair, .. } => Value::Word(regs[pair].pair().0),
            Operation::High { pair, .. } => Value::Word(regs[pair].pair().1),
            Operation::Offset { base, bytes } => {
                Value::Word((regs[base].word() as i64 + *bytes) as u64)
            }
            other => self.memory(other, out, regs),
        }
    }

    fn memory(&mut self, op: &Operation, out: &Type, regs: &HashMap<u32, Value>) -> Value {
        match op {
            Operation::Load { address, kind, .. } => {
                Value::decode(&self.read(regs[address].word(), width(kind)), kind)
            }
            Operation::Store {
                address,
                value,
                kind,
                ..
            } => {
                let bytes = regs[value].encode(kind);
                self.write(regs[address].word(), &bytes);
                Value::Void
            }
            Operation::Copy {
                destination,
                source,
                bytes,
            } => {
                let data = self.read(regs[source].word(), *bytes as usize);
                self.write(regs[destination].word(), &data);
                Value::Void
            }
            Operation::Call {
                symbol, arguments, ..
            } => {
                let args = arguments.iter().map(|id| regs[id].clone()).collect();
                self.invoke(symbol, args)
            }
            Operation::Indirect {
                callee, arguments, ..
            } => {
                let ptr = regs[callee].word();
                let sym = self.names[&ptr].clone();
                let args = arguments.iter().map(|id| regs[id].clone()).collect();
                self.invoke(&sym, args)
            }
            _ => {
                let _ = out;
                Value::Void
            }
        }
    }
}

enum Step {
    Jump(usize),
    Return(Value),
}

fn release(blocks: &mut HashMap<u64, Vec<u8>>, owned: &[u64]) {
    for addr in owned {
        let _ = blocks.remove(addr);
    }
}

fn jump(
    map: &HashMap<u32, usize>,
    func: &Function,
    edge: &Target,
    regs: &mut HashMap<u32, Value>,
) -> usize {
    let dst = map[&edge.block];
    let vals: Vec<Value> = edge.arguments.iter().map(|id| regs[id].clone()).collect();
    for ((pid, _), val) in func.blocks[dst].parameters.iter().zip(vals) {
        regs.insert(*pid, val);
    }
    dst
}

fn width(kind: &Type) -> usize {
    match kind {
        Type::Void => 0,
        Type::Integer { bits, .. } => (*bits as usize).div_ceil(8),
        Type::Float { bits } => (*bits as usize) / 8,
        Type::Pointer => 8,
        Type::Pair => 16,
        Type::Aggregate { size, .. } => *size as usize,
    }
}
