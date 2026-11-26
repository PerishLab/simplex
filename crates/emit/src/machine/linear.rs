use crate::encode::media::{Arithmetic, Precision};
use crate::encode::{Condition, Label, Shift, Width};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub bytes: u32,
    pub align: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub width: Width,
    pub signed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binary {
    Add,
    Sub,
    Mul,
    Quotient(bool),
    Remainder(bool),
    Conjunction,
    Disjunction,
    Exclusive,
    Shift(Shift),
    Compare(Condition),
    Float(Arithmetic, Precision),
    Order(&'static str, Precision),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unary {
    Negate,
    Invert,
    Not,
    Extend(Width),
    Widen(Width),
    Promote(Precision),
    Unsigned(Precision),
    Truncate(Precision),
    Resize(Precision),
    Flip(Precision),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Callee {
    Direct { symbol: String, external: bool },
    Indirect(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outgoing {
    Scalar { src: u32, offset: u32 },
    Block { base: u32, offset: u32, bytes: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub callee: Callee,
    pub registers: Vec<u32>,
    pub floats: Vec<(u32, Precision)>,
    pub stack: Vec<Outgoing>,
    pub space: u32,
    pub first: Option<u32>,
    pub second: Option<u32>,
    pub media: Option<Precision>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Void,
    Single(u32),
    Pair(u32, u32),
    Indirect {
        target: u32,
        source: u32,
        bytes: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parameter {
    Register { dst: u32, index: usize },
    Stack { dst: u32, offset: i32 },
    Block { slot: Slot, offset: i32, bytes: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Mark(Label),
    Constant {
        dst: u32,
        value: i64,
    },
    Move {
        dst: u32,
        src: u32,
    },
    Slot {
        dst: u32,
        slot: Slot,
        delta: i32,
    },
    Symbol {
        dst: u32,
        name: String,
        external: bool,
    },
    Load {
        dst: u32,
        base: u32,
        offset: i32,
        access: Access,
    },
    Store {
        base: u32,
        offset: i32,
        src: u32,
        width: Width,
    },
    Copy {
        dst: u32,
        src: u32,
        bytes: u32,
    },
    Zero {
        dst: u32,
        bytes: u32,
    },
    Binary {
        kind: Binary,
        dst: u32,
        lhs: u32,
        rhs: u32,
    },
    Unary {
        kind: Unary,
        dst: u32,
        src: u32,
    },
    Jump(Label),
    Branch {
        cond: u32,
        truth: bool,
        target: Label,
    },
    Invoke(Invocation),
    Finish(Ending),
}

#[derive(Clone, Debug, Default)]
pub struct Procedure {
    pub name: String,
    pub values: u32,
    pub labels: u32,
    pub slots: Vec<Allocation>,
    pub parameters: Vec<Parameter>,
    pub steps: Vec<Step>,
}

impl Procedure {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }

    pub fn value(&mut self) -> u32 {
        let index = self.values;
        self.values += 1;
        index
    }

    pub fn label(&mut self) -> Label {
        let index = self.labels;
        self.labels += 1;
        Label(index)
    }

    pub fn reserve(&mut self, bytes: u32, align: u32) -> Slot {
        let index = self.slots.len() as u32;
        self.slots.push(Allocation {
            bytes: bytes.max(8),
            align: align.max(8),
        });
        Slot(index)
    }

    pub fn emit(&mut self, step: Step) {
        self.steps.push(step);
    }
}
