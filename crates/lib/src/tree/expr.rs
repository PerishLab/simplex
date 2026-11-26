use super::{Generic, Item, Parameter, Spec};

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
    List {
        items: Vec<Expression>,
        multiline: bool,
        line: usize,
        column: usize,
    },
    Unary {
        operator: String,
        operand: Box<Expression>,
    },
    Borrow {
        mutable: bool,
        target: Box<Expression>,
        line: usize,
        column: usize,
    },
    Index {
        base: Box<Expression>,
        start: Box<Expression>,
        limit: Option<Box<Expression>>,
        line: usize,
        column: usize,
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
        generics: Vec<Spec>,
        arguments: Vec<Expression>,
        multiline: bool,
        line: usize,
        column: usize,
    },
    Meta(Block),
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
        asynchronous: bool,
        generics: Vec<Generic>,
        inputs: Vec<Parameter>,
        output: Option<Spec>,
        body: Body,
    },
}
