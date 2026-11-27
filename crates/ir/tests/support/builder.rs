use simplex_ir::{
    Binary, Block, Cast, External, Function, Instruction, Linkage, Operation, Target, Terminator,
    Type,
};

pub const UNSIGNED: Type = Type::Integer {
    bits: 64,
    signed: false,
};
pub const SIGNED: Type = Type::Integer {
    bits: 64,
    signed: true,
};
pub const BYTE: Type = Type::Integer {
    bits: 8,
    signed: false,
};
pub const FLAG: Type = Type::Integer {
    bits: 1,
    signed: false,
};

pub fn emitter() -> External {
    External {
        symbol: "emit".to_owned(),
        linkage: Linkage::System,
        inputs: vec![BYTE],
        output: Type::Void,
    }
}

pub struct Maker {
    pub func: Function,
    pub active: usize,
}

impl Maker {
    pub fn new(symbol: &str, asynchronous: bool, output: Type) -> Self {
        Self {
            func: Function {
                symbol: symbol.to_owned(),
                linkage: Linkage::Exported,
                asynchronous,
                cleanup: None,
                inputs: Vec::new(),
                output,
                values: 0,
                blocks: Vec::new(),
            },
            active: 0,
        }
    }

    pub fn cleanup(&mut self, symbol: &str) {
        self.func.cleanup = Some(symbol.to_owned());
    }

    pub fn input(&mut self, kind: Type) -> u32 {
        let id = self.func.values;
        self.func.values += 1;
        self.func.inputs.push((id, kind));
        id
    }

    pub fn block(&mut self) -> u32 {
        let id = self.func.blocks.len() as u32;
        self.func.blocks.push(Block {
            id,
            parameters: Vec::new(),
            body: Vec::new(),
            exit: Terminator::Trap { code: 0 },
        });
        id
    }

    pub fn parameter(&mut self, block: u32, kind: Type) -> u32 {
        let id = self.func.values;
        self.func.values += 1;
        self.func.blocks[block as usize].parameters.push((id, kind));
        id
    }

    pub fn select(&mut self, block: u32) {
        self.active = block as usize;
    }

    pub fn push(&mut self, kind: Type, op: Operation) -> u32 {
        let id = self.func.values;
        self.func.values += 1;
        self.func.blocks[self.active].body.push(Instruction {
            value: id,
            kind,
            op,
        });
        id
    }

    pub fn byte(&mut self, ch: u8) -> u32 {
        self.push(
            BYTE,
            Operation::Integer {
                value: i128::from(ch),
                kind: BYTE,
            },
        )
    }

    pub fn unsigned(&mut self, val: u64) -> u32 {
        self.push(
            UNSIGNED,
            Operation::Integer {
                value: i128::from(val),
                kind: UNSIGNED,
            },
        )
    }

    pub fn slot(&mut self, size: u32) -> u32 {
        self.push(Type::Pointer, Operation::Slot { size, align: 8 })
    }

    pub fn offset(&mut self, base: u32, bytes: i64) -> u32 {
        self.push(Type::Pointer, Operation::Offset { base, bytes })
    }

    pub fn load(&mut self, address: u32, kind: Type) -> u32 {
        self.push(
            kind.clone(),
            Operation::Load {
                address,
                kind,
                volatile: false,
            },
        )
    }

    pub fn store(&mut self, address: u32, value: u32, kind: Type) {
        let _ = self.push(
            Type::Void,
            Operation::Store {
                address,
                value,
                kind,
                volatile: false,
            },
        );
    }

    pub fn mark(&mut self, value: u32) {
        let _ = self.push(
            Type::Void,
            Operation::Call {
                symbol: "emit".to_owned(),
                arguments: vec![value],
                inputs: vec![BYTE],
                kind: Type::Void,
            },
        );
    }

    pub fn pause(&mut self) {
        let _ = self.push(Type::Void, Operation::Yield);
    }

    pub fn spawn(&mut self, symbol: &str, args: Vec<u32>, inputs: Vec<Type>, result: Type) -> u32 {
        self.push(
            Type::Pointer,
            Operation::Spawn {
                symbol: symbol.to_owned(),
                arguments: args,
                inputs,
                result,
            },
        )
    }

    pub fn binary(&mut self, op: Binary, left: u32, right: u32, kind: Type) -> u32 {
        let out = match matches!(
            op,
            Binary::Eq | Binary::Ne | Binary::Lt | Binary::Le | Binary::Gt | Binary::Ge
        ) {
            true => FLAG,
            false => kind.clone(),
        };
        self.push(
            out,
            Operation::Binary {
                op,
                left,
                right,
                kind,
            },
        )
    }

    pub fn signed(&mut self, value: u32) -> u32 {
        self.push(
            SIGNED,
            Operation::Cast {
                op: Cast::Bitcast,
                value,
                from: UNSIGNED,
                kind: SIGNED,
            },
        )
    }

    pub fn ret(&mut self, val: Option<u32>) {
        self.func.blocks[self.active].exit = Terminator::Return(val);
    }

    pub fn jump(&mut self, block: u32, arguments: Vec<u32>) {
        self.func.blocks[self.active].exit = Terminator::Jump(Target { block, arguments });
    }

    pub fn branch(&mut self, condition: u32, truthy: (u32, Vec<u32>), falsy: (u32, Vec<u32>)) {
        self.func.blocks[self.active].exit = Terminator::Branch {
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
        self.func
    }
}
