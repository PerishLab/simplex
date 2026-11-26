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
    Bind(Binding),
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
pub struct Binding {
    pub mutable: bool,
    pub name: String,
    pub annotation: Option<Spec>,
    pub value: Expression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spec {
    Named {
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
    Word(String),
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
    },
    Member {
        base: Box<Expression>,
        field: String,
        generics: Vec<Spec>,
        wrapped: bool,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
        multiline: bool,
    },
    Record {
        name: String,
        fields: Vec<Field>,
        multiline: bool,
    },
    Try(Box<Expression>),
    Switch(Box<Dispatch>),
    Bucket(Box<Partition>),
    Loop(Box<Loop>),
    Function {
        inputs: Vec<Parameter>,
        output: Option<Spec>,
        body: Body,
    },
}
