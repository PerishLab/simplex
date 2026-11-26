mod expr;

pub use expr::{
    Block, Body, Branch, Dispatch, Expression, Field, Loop, Outcome, Partition, Pattern,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub statement: Statement,
    pub blank: bool,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    Import(Import),
    Extern(External),
    Bind(Binding),
    Struct(Structure),
    Enum(Enumeration),
    Interface(Contract),
    Alias(Alias),
    Conform(Conformance),
    Assign {
        target: Expression,
        operator: String,
        value: Expression,
    },
    For {
        constant: bool,
        item: String,
        source: Expression,
        body: Block,
    },
    Loop(Loop),
    Evaluate(Expression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub dynamic: bool,
    pub name: String,
    pub path: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub public: bool,
    pub dynamic: bool,
    pub name: String,
    pub abi: String,
    pub symbol: String,
    pub inputs: Vec<Parameter>,
    pub output: Option<Spec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decorator {
    pub module: Option<String>,
    pub name: String,
    pub arguments: Vec<Expression>,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generic {
    pub name: String,
    pub bounds: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub decorators: Vec<Decorator>,
    pub public: bool,
    pub name: String,
    pub spec: Spec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structure {
    pub decorators: Vec<Decorator>,
    pub public: bool,
    pub dynamic: bool,
    pub name: String,
    pub generics: Vec<Generic>,
    pub fields: Vec<Slot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub decorators: Vec<Decorator>,
    pub name: String,
    pub payloads: Vec<Spec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enumeration {
    pub decorators: Vec<Decorator>,
    pub public: bool,
    pub dynamic: bool,
    pub name: String,
    pub generics: Vec<Generic>,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub name: String,
    pub asynchronous: bool,
    pub inputs: Vec<Parameter>,
    pub output: Option<Spec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contract {
    pub public: bool,
    pub dynamic: bool,
    pub name: String,
    pub generics: Vec<Generic>,
    pub methods: Vec<Requirement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alias {
    pub public: bool,
    pub dynamic: bool,
    pub name: String,
    pub generics: Vec<Generic>,
    pub target: Spec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub value: Expression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conformance {
    pub subject: String,
    pub contract: Spec,
    pub methods: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub decorators: Vec<Decorator>,
    pub public: bool,
    pub dynamic: bool,
    pub mutable: bool,
    pub name: String,
    pub annotation: Option<Spec>,
    pub value: Expression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spec {
    Named {
        module: Option<String>,
        name: String,
        arguments: Vec<Spec>,
    },
    Borrow {
        mutable: bool,
        target: Box<Spec>,
    },
    Slice(Box<Spec>),
    Callable {
        asynchronous: bool,
        inputs: Vec<Spec>,
        output: Option<Box<Spec>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub constant: bool,
    pub name: String,
    pub annotation: Option<Spec>,
    pub line: usize,
    pub column: usize,
}
