#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Register {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl Register {
    pub const fn low(self) -> u8 {
        (self as u8) & 7
    }

    pub const fn high(self) -> u8 {
        ((self as u8) >> 3) & 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Condition {
    Limit = 0x3,
    Equal = 0x4,
    Different = 0x5,
}

struct Patch {
    site: usize,
    target: usize,
}

pub struct Assembler {
    bytes: Vec<u8>,
    marks: Vec<usize>,
    patches: Vec<Patch>,
}

impl Assembler {
    pub fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(256),
            marks: Vec::new(),
            patches: Vec::new(),
        }
    }

    pub fn label(&mut self) -> usize {
        let index = self.marks.len();
        self.marks.push(usize::MAX);
        index
    }

    pub fn bind(&mut self, label: usize) {
        self.marks[label] = self.bytes.len();
    }

    pub fn finish(mut self) -> Vec<u8> {
        for patch in self.patches {
            let target = self.marks[patch.target] as isize;
            let origin = (patch.site + 4) as isize;
            let delta = ((target - origin) as i32).to_le_bytes();
            self.bytes[patch.site..patch.site + 4].copy_from_slice(&delta);
        }
        self.bytes
    }

    fn rex(&mut self, wide: bool, reg: u8, base: u8) {
        let flag = 0x40 | ((wide as u8) << 3) | (reg << 2) | base;
        self.bytes.push(flag);
    }

    fn memory(&mut self, reg: u8, base: Register, offset: i32) {
        let low = base.low();
        let mode = match (offset, low) {
            (0, 0..=4 | 6..=7) => 0x00,
            (-128..=127, _) => 0x40,
            _ => 0x80,
        };
        self.bytes.push(mode | (reg << 3) | low);
        if low == 4 {
            self.bytes.push(0x24);
        }
        match mode {
            0x00 => {}
            0x40 => self.bytes.push(offset as i8 as u8),
            _ => self.bytes.extend_from_slice(&offset.to_le_bytes()),
        }
    }

    fn displacement(&mut self, target: usize) {
        let site = self.bytes.len();
        self.bytes.extend_from_slice(&[0; 4]);
        self.patches.push(Patch { site, target });
    }

    pub fn system(&mut self) {
        self.bytes.extend_from_slice(&[0x0F, 0x05]);
    }

    pub fn trap(&mut self) {
        self.bytes.extend_from_slice(&[0x0F, 0x0B]);
    }

    pub fn leave(&mut self) {
        self.bytes.push(0xC3);
    }

    pub fn push(&mut self, reg: Register) {
        if reg.high() != 0 {
            self.bytes.push(0x41);
        }
        self.bytes.push(0x50 + reg.low());
    }

    pub fn pop(&mut self, reg: Register) {
        if reg.high() != 0 {
            self.bytes.push(0x41);
        }
        self.bytes.push(0x58 + reg.low());
    }

    pub fn copy(&mut self, dst: Register, src: Register) {
        self.rex(true, src.high(), dst.high());
        self.bytes.push(0x89);
        self.bytes.push(0xC0 | (src.low() << 3) | dst.low());
    }

    pub fn zero(&mut self, reg: Register) {
        if reg.high() != 0 {
            self.rex(false, reg.high(), reg.high());
        }
        self.bytes.push(0x31);
        self.bytes.push(0xC0 | (reg.low() << 3) | reg.low());
    }

    pub fn constant(&mut self, dst: Register, value: u32) {
        if dst.high() != 0 {
            self.bytes.push(0x41);
        }
        self.bytes.push(0xB8 + dst.low());
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn signed(&mut self, dst: Register, value: i32) {
        self.rex(true, 0, dst.high());
        self.bytes.push(0xC7);
        self.bytes.push(0xC0 | dst.low());
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn load(&mut self, dst: Register, base: Register, offset: i32) {
        self.rex(true, dst.high(), base.high());
        self.bytes.push(0x8B);
        self.memory(dst.low(), base, offset);
    }

    pub fn store(&mut self, base: Register, offset: i32, src: Register) {
        self.rex(true, src.high(), base.high());
        self.bytes.push(0x89);
        self.memory(src.low(), base, offset);
    }

    pub fn put(&mut self, base: Register, offset: i32, value: i32) {
        self.rex(true, 0, base.high());
        self.bytes.push(0xC7);
        self.memory(0, base, offset);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn indexed(&mut self, dst: Register, base: Register, index: Register) {
        let flag = 0x48 | (dst.high() << 2) | (index.high() << 1) | base.high();
        self.bytes.push(flag);
        self.bytes.push(0x8B);
        let sib = 0xC0 | (index.low() << 3) | base.low();
        match base.low() {
            5 => self
                .bytes
                .extend_from_slice(&[0x44 | (dst.low() << 3), sib, 0]),
            _ => self
                .bytes
                .extend_from_slice(&[0x04 | (dst.low() << 3), sib]),
        }
    }

    pub fn address(&mut self, dst: Register, base: Register, offset: i32) {
        self.rex(true, dst.high(), base.high());
        self.bytes.push(0x8D);
        self.memory(dst.low(), base, offset);
    }

    pub fn anchor(&mut self, dst: Register, target: usize) {
        self.rex(true, dst.high(), 0);
        self.bytes.push(0x8D);
        self.bytes.push(0x05 | (dst.low() << 3));
        self.displacement(target);
    }

    pub fn add(&mut self, dst: Register, amount: i8) {
        self.rex(true, 0, dst.high());
        self.bytes
            .extend_from_slice(&[0x83, 0xC0 | dst.low(), amount as u8]);
    }

    pub fn subtract(&mut self, dst: Register, amount: i8) {
        self.rex(true, 0, dst.high());
        self.bytes
            .extend_from_slice(&[0x83, 0xE8 | dst.low(), amount as u8]);
    }

    pub fn shift(&mut self, dst: Register, bits: u8) {
        self.rex(true, 0, dst.high());
        self.bytes
            .extend_from_slice(&[0xC1, 0xE0 | dst.low(), bits]);
    }

    pub fn plus(&mut self, dst: Register, src: Register) {
        self.rex(true, src.high(), dst.high());
        self.bytes.push(0x01);
        self.bytes.push(0xC0 | (src.low() << 3) | dst.low());
    }

    pub fn test(&mut self, reg: Register) {
        self.rex(true, reg.high(), reg.high());
        self.bytes.push(0x85);
        self.bytes.push(0xC0 | (reg.low() << 3) | reg.low());
    }

    pub fn compare(&mut self, lhs: Register, rhs: Register) {
        self.rex(true, rhs.high(), lhs.high());
        self.bytes.push(0x39);
        self.bytes.push(0xC0 | (rhs.low() << 3) | lhs.low());
    }

    pub fn check(&mut self, base: Register, offset: i32, value: i8) {
        self.rex(true, 0, base.high());
        self.bytes.push(0x83);
        self.memory(7, base, offset);
        self.bytes.push(value as u8);
    }

    pub fn limit(&mut self, reg: Register, value: i32) {
        self.rex(true, 0, reg.high());
        self.bytes.push(0x81);
        self.bytes.push(0xF8 | reg.low());
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn jump(&mut self, target: usize) {
        self.bytes.push(0xE9);
        self.displacement(target);
    }

    pub fn branch(&mut self, cond: Condition, target: usize) {
        self.bytes.extend_from_slice(&[0x0F, 0x80 + (cond as u8)]);
        self.displacement(target);
    }

    pub fn invoke(&mut self, target: usize) {
        self.bytes.push(0xE8);
        self.displacement(target);
    }

    pub fn indirect(&mut self, reg: Register) {
        if reg.high() != 0 {
            self.bytes.push(0x41);
        }
        self.bytes.extend_from_slice(&[0xFF, 0xD0 | reg.low()]);
    }

    pub fn transfer(&mut self, reg: Register) {
        if reg.high() != 0 {
            self.bytes.push(0x41);
        }
        self.bytes.extend_from_slice(&[0xFF, 0xE0 | reg.low()]);
    }
}
