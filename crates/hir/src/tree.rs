use crate::layout::{Catalog, Field, Type};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub local: String,
    pub symbol: String,
    pub library: Option<String>,
    pub inputs: Vec<Type>,
    pub output: Type,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub symbol: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub symbol: String,
    pub subject: String,
    pub contract: String,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub tags: Vec<u64>,
    pub bindings: Vec<(String, Field)>,
    pub body: Expression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    Void,
    Int(i128, Type),
    Float(u64, Type),
    Bool(bool),
    Bytes(String),
    Symbol(String, Type),
    Local(String, Type),
    Unary {
        operator: String,
        operand: Box<Expression>,
        kind: Type,
    },
    Binary {
        operator: String,
        left: Box<Expression>,
        right: Box<Expression>,
        kind: Type,
    },
    Cast {
        value: Box<Expression>,
        kind: Type,
    },
    Borrow {
        mutable: bool,
        target: Box<Expression>,
        kind: Type,
    },
    Load {
        pointer: Box<Expression>,
        kind: Type,
    },
    Field {
        base: Box<Expression>,
        name: String,
        offset: u64,
        kind: Type,
    },
    Index {
        base: Box<Expression>,
        index: Box<Expression>,
        kind: Type,
    },
    Window {
        base: Box<Expression>,
        start: Box<Expression>,
        limit: Box<Expression>,
        kind: Type,
    },
    Length(Box<Expression>),
    Record {
        name: String,
        fields: Vec<(String, Expression)>,
        kind: Type,
    },
    Variant {
        choice: String,
        arm: String,
        tag: u64,
        payloads: Vec<Expression>,
        kind: Type,
    },
    List {
        items: Vec<Expression>,
        element: Type,
        kind: Type,
    },
    Buffer {
        count: Box<Expression>,
        element: Type,
        kind: Type,
    },
    Coerce {
        value: Box<Expression>,
        table: String,
        kind: Type,
    },
    Call {
        symbol: String,
        arguments: Vec<Expression>,
        kind: Type,
    },
    Virtual {
        receiver: Box<Expression>,
        slot: usize,
        arguments: Vec<Expression>,
        kind: Type,
    },
    Indirect {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
        kind: Type,
    },
    Block {
        body: Vec<Statement>,
        tail: Option<Box<Expression>>,
        kind: Type,
    },
    Branch {
        condition: Box<Expression>,
        then: Box<Expression>,
        otherwise: Box<Expression>,
        kind: Type,
    },
    Switch {
        subject: Box<Expression>,
        cases: Vec<Case>,
        kind: Type,
    },
    Loop {
        body: Vec<Statement>,
        kind: Type,
    },
    Yield,
    Await {
        fiber: Box<Expression>,
        kind: Type,
    },
    All {
        fibers: Box<Expression>,
        element: Type,
        kind: Type,
    },
    Race {
        fibers: Box<Expression>,
        kind: Type,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    Bind {
        name: String,
        mutable: bool,
        kind: Type,
        value: Expression,
    },
    Assign {
        name: String,
        value: Expression,
    },
    Store {
        target: Expression,
        value: Expression,
    },
    Drop {
        name: String,
        symbol: String,
        borrowed: bool,
    },
    Break(Option<Expression>),
    Continue,
    Evaluate(Expression),
    Result(Expression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Routine {
    pub symbol: String,
    pub exported: bool,
    pub asynchronous: bool,
    pub inputs: Vec<(String, Type)>,
    pub output: Type,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub namespace: String,
    pub shared: bool,
    pub libraries: Vec<String>,
    pub imports: Vec<Import>,
    pub assets: Vec<Asset>,
    pub tables: Vec<Table>,
    pub catalog: Catalog,
    pub routines: Vec<Routine>,
    pub entry: Option<String>,
}
