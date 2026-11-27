use crate::{
    Binary, Block, Cast, Function, Instruction, Linkage, Operation, Target, Terminator, Type,
};

pub const UNSIGNED: Type = Type::Integer {
    bits: 64,
    signed: false,
};
pub const SIGNED: Type = Type::Integer {
    bits: 64,
    signed: true,
};
pub const BOOLEAN: Type = Type::Integer {
    bits: 1,
    signed: false,
};
pub const BYTE: Type = Type::Integer {
    bits: 8,
    signed: false,
};

pub struct Builder {
    pub symbol: String,
    pub linkage: Linkage,
    pub inputs: Vec<(u32, Type)>,
    pub output: Type,
    pub values: u32,
    pub blocks: Vec<Block>,
    pub active: usize,
}

impl Builder {
    pub fn new(symbol: String, linkage: Linkage, output: Type) -> Self {
        Self {
            symbol,
            linkage,
            inputs: Vec::new(),
            output,
            values: 0,
            blocks: Vec::new(),
            active: 0,
        }
    }

    pub fn fresh(&mut self) -> u32 {
        let id = self.values;
        self.values += 1;
        id
    }

    pub fn input(&mut self, kind: Type) -> u32 {
        let id = self.fresh();
        self.inputs.push((id, kind));
        id
    }

    pub fn block(&mut self) -> u32 {
        let id = self.blocks.len() as u32;
        self.blocks.push(Block {
            id,
            parameters: Vec::new(),
            body: Vec::new(),
            exit: Terminator::Trap { code: 0 },
        });
        id
    }

    pub fn parameter(&mut self, block: u32, kind: Type) -> u32 {
        let id = self.fresh();
        self.blocks[block as usize].parameters.push((id, kind));
        id
    }

    pub fn select(&mut self, block: u32) {
        self.active = block as usize;
    }

    pub fn emit(&mut self, kind: Type, op: Operation) -> u32 {
        let id = self.fresh();
        self.blocks[self.active].body.push(Instruction {
            value: id,
            kind,
            op,
        });
        id
    }

    pub fn void(&mut self, op: Operation) {
        let _ = self.emit(Type::Void, op);
    }

    pub fn unsigned(&mut self, value: u64) -> u32 {
        self.emit(
            UNSIGNED,
            Operation::Integer {
                value: i128::from(value),
                kind: UNSIGNED,
            },
        )
    }

    pub fn signed(&mut self, value: i64) -> u32 {
        self.emit(
            SIGNED,
            Operation::Integer {
                value: i128::from(value),
                kind: SIGNED,
            },
        )
    }

    pub fn null(&mut self) -> u32 {
        let zero = self.unsigned(0);
        self.bitcast(zero, UNSIGNED, Type::Pointer)
    }

    pub fn symbol(&mut self, name: &str) -> u32 {
        self.emit(
            Type::Pointer,
            Operation::Symbol {
                name: name.to_owned(),
            },
        )
    }

    pub fn offset(&mut self, base: u32, bytes: i64) -> u32 {
        if bytes == 0 {
            return base;
        }
        self.emit(Type::Pointer, Operation::Offset { base, bytes })
    }

    pub fn advance(&mut self, base: u32, delta: u32) -> u32 {
        let raw = self.bitcast(base, Type::Pointer, UNSIGNED);
        let sum = self.binary(Binary::Add, raw, delta, UNSIGNED);
        self.bitcast(sum, UNSIGNED, Type::Pointer)
    }

    pub fn load(&mut self, address: u32, kind: Type) -> u32 {
        self.emit(
            kind.clone(),
            Operation::Load {
                address,
                kind,
                volatile: false,
            },
        )
    }

    pub fn store(&mut self, address: u32, value: u32, kind: Type) {
        self.void(Operation::Store {
            address,
            value,
            kind,
            volatile: false,
        });
    }

    pub fn bitcast(&mut self, value: u32, from: Type, kind: Type) -> u32 {
        self.emit(
            kind.clone(),
            Operation::Cast {
                op: Cast::Bitcast,
                value,
                from,
                kind,
            },
        )
    }

    pub fn binary(&mut self, op: Binary, left: u32, right: u32, kind: Type) -> u32 {
        let out = match op.comparison() {
            true => BOOLEAN,
            false => kind.clone(),
        };
        self.emit(
            out,
            Operation::Binary {
                op,
                left,
                right,
                kind,
            },
        )
    }

    pub fn isnull(&mut self, pointer: u32) -> u32 {
        let raw = self.bitcast(pointer, Type::Pointer, UNSIGNED);
        let zero = self.unsigned(0);
        self.binary(Binary::Eq, raw, zero, UNSIGNED)
    }

    pub fn call(
        &mut self,
        symbol: &str,
        arguments: Vec<u32>,
        inputs: Vec<Type>,
        kind: Type,
    ) -> u32 {
        let op = Operation::Call {
            symbol: symbol.to_owned(),
            arguments,
            inputs,
            kind: kind.clone(),
        };
        self.emit(kind, op)
    }

    pub fn indirect(
        &mut self,
        callee: u32,
        arguments: Vec<u32>,
        inputs: Vec<Type>,
        kind: Type,
    ) -> u32 {
        let op = Operation::Indirect {
            callee,
            arguments,
            inputs,
            kind: kind.clone(),
        };
        self.emit(kind, op)
    }

    pub fn jump(&mut self, block: u32, arguments: Vec<u32>) {
        self.blocks[self.active].exit = Terminator::Jump(Target { block, arguments });
    }

    pub fn branch(&mut self, condition: u32, truthy: (u32, Vec<u32>), falsy: (u32, Vec<u32>)) {
        self.blocks[self.active].exit = Terminator::Branch {
            condition,
            truthy: Target {
                block: truthy.0,
                arguments: truthy.1,
            },
            falsy: Target {
                block: falsy.0,
                arguments: falsy.1,
            },
        };
    }

    pub fn finish(self) -> Function {
        Function {
            symbol: self.symbol,
            linkage: self.linkage,
            asynchronous: false,
            cleanup: None,
            inputs: self.inputs,
            output: self.output,
            values: self.values,
            blocks: self.blocks,
        }
    }
}
