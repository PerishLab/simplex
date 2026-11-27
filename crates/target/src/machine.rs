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

pub fn stretch(spans: &mut [Span], loops: &[(u32, u32)], calls: &[u32]) {
    let mut changed = true;
    while changed {
        changed = false;
        for &(header, latch) in loops {
            changed |= extend(spans, header, latch);
        }
    }
    for span in spans.iter_mut() {
        if calls
            .iter()
            .any(|&point| span.start < point && span.end > point)
        {
            span.across = true;
        }
    }
}

fn extend(spans: &mut [Span], header: u32, latch: u32) -> bool {
    let mut updated = false;
    for span in spans.iter_mut() {
        if span.start < header && span.end >= header && span.end < latch {
            span.end = latch;
            updated = true;
        }
    }
    updated
}
