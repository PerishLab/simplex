#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Void,
    Integer { bits: u8, signed: bool },
    Float { bits: u8 },
    Pointer,
    Pair,
    Aggregate { size: u32, align: u32, packed: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Relaxed,
    Acquire,
    Release,
    Sequential,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binary {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unary {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cast {
    Widen,
    Narrow,
    Promote,
    Elevate,
    Truncate,
    Demote,
    Extend,
    Shrink,
    Bitcast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Atomic {
    Exchange,
    Add,
    Sub,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Linkage {
    Internal,
    Exported,
    System,
    Dynamic(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub symbol: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub symbol: String,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub symbol: String,
    pub linkage: Linkage,
    pub inputs: Vec<Type>,
    pub output: Type,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Integer {
        value: i128,
        kind: Type,
    },
    Float {
        bits: u64,
        kind: Type,
    },
    Symbol {
        name: String,
    },
    Slot {
        size: u32,
        align: u32,
    },
    Unary {
        op: Unary,
        value: u32,
        kind: Type,
    },
    Binary {
        op: Binary,
        left: u32,
        right: u32,
        kind: Type,
    },
    Cast {
        op: Cast,
        value: u32,
        from: Type,
        kind: Type,
    },
    Pair {
        low: u32,
        high: u32,
    },
    Low {
        pair: u32,
        kind: Type,
    },
    High {
        pair: u32,
        kind: Type,
    },
    Offset {
        base: u32,
        bytes: i64,
    },
    Load {
        address: u32,
        kind: Type,
        volatile: bool,
    },
    Store {
        address: u32,
        value: u32,
        kind: Type,
        volatile: bool,
    },
    Copy {
        destination: u32,
        source: u32,
        bytes: u32,
    },
    Acquire {
        address: u32,
        kind: Type,
        order: Order,
    },
    Release {
        address: u32,
        value: u32,
        kind: Type,
        order: Order,
    },
    Swap {
        address: u32,
        expected: u32,
        desired: u32,
        kind: Type,
        order: Order,
    },
    Atomic {
        op: Atomic,
        address: u32,
        operand: u32,
        kind: Type,
        order: Order,
    },
    Fence {
        order: Order,
    },
    Call {
        symbol: String,
        arguments: Vec<u32>,
        inputs: Vec<Type>,
        kind: Type,
    },
    Indirect {
        callee: u32,
        arguments: Vec<u32>,
        inputs: Vec<Type>,
        kind: Type,
    },
    Spawn {
        symbol: String,
        arguments: Vec<u32>,
        inputs: Vec<Type>,
        result: Type,
    },
    Yield,
    Await {
        fiber: u32,
        kind: Type,
    },
    All {
        fibers: u32,
        count: u32,
        element: Type,
    },
    Race {
        fibers: u32,
        count: u32,
        kind: Type,
    },
}
