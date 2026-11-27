#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub value: u32,
    pub start: u32,
    pub end: u32,
    pub float: bool,
    pub across: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bank {
    pub general: Vec<u8>,
    pub preserved: Vec<u8>,
    pub float: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    General(u8),
    Float(u8),
    Spill(i32),
}
