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
        item: String,
        source: Expression,
        body: Block,
    },
    Loop(Loop),
    Evaluate(Expression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub name: String,
    pub path: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub public: bool,
    pub name: String,
    pub abi: String,
    pub symbol: String,
    pub inputs: Vec<Parameter>,
    pub output: Option<Spec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generic {
    pub name: String,
    pub bounds: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub public: bool,
    pub name: String,
    pub spec: Spec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structure {
    pub public: bool,
    pub name: String,
    pub generics: Vec<Generic>,
    pub fields: Vec<Slot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub payloads: Vec<Spec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enumeration {
    pub public: bool,
    pub name: String,
    pub generics: Vec<Generic>,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub name: String,
    pub inputs: Vec<Parameter>,
    pub output: Option<Spec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contract {
    pub public: bool,
    pub name: String,
    pub generics: Vec<Generic>,
    pub methods: Vec<Requirement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alias {
    pub public: bool,
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
    pub public: bool,
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
    Callable {
        inputs: Vec<Spec>,
        output: Option<Box<Spec>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub annotation: Option<Spec>,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Block(Block),
    Switch(Box<Dispatch>),
    Bucket(Box<Partition>),
    Loop(Box<Loop>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub items: Vec<Item>,
    pub compact: bool,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loop {
    Block(Block),
    Switch(Box<Dispatch>),
    Bucket(Box<Partition>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub subject: Expression,
    pub arms: Vec<Branch>,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    pub subject: Option<Expression>,
    pub arms: Vec<Branch>,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub pattern: Pattern,
    pub outcome: Outcome,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pattern {
    Wildcard,
    Choices(Vec<Expression>),
    Range { start: Expression, end: Expression },
    Predicate(Expression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Value(Expression),
    Break(Option<Expression>),
    Continue,
    Block(Block),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub value: Expression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    Word {
        text: String,
        line: usize,
        column: usize,
    },
    Number(String),
    Text(String),
    Byte(String),
    Group(Box<Expression>),
    Unary {
        operator: String,
        operand: Box<Expression>,
    },
    Binary {
        left: Box<Expression>,
        operator: String,
        right: Box<Expression>,
        wrapped: bool,
        line: usize,
        column: usize,
    },
    Member {
        base: Box<Expression>,
        field: String,
        generics: Vec<Spec>,
        wrapped: bool,
        line: usize,
        column: usize,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
        multiline: bool,
        line: usize,
        column: usize,
    },
    Record {
        module: Option<String>,
        name: String,
        fields: Vec<Field>,
        multiline: bool,
        line: usize,
        column: usize,
    },
    Try(Box<Expression>),
    Switch(Box<Dispatch>),
    Bucket(Box<Partition>),
    Loop(Box<Loop>),
    Function {
        generics: Vec<Generic>,
        inputs: Vec<Parameter>,
        output: Option<Spec>,
        body: Body,
    },
}
