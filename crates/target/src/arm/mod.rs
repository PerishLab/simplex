pub mod integer;
pub mod lower;
pub mod memory;
pub mod scalar;
#[cfg(test)]
mod tests;

pub use lower::{Resolver, generate};

use crate::Fault;
use simplex_object::{Fixup, Relocation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Label(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Byte,
    Half,
    Word,
    Quad,
}

impl Width {
    pub const fn from(bytes: u32) -> Self {
        match bytes {
            1 => Self::Byte,
            2 => Self::Half,
            4 => Self::Word,
            _ => Self::Quad,
        }
    }

    pub const fn scale(self) -> u8 {
        match self {
            Self::Byte => 0,
            Self::Half => 1,
            Self::Word => 2,
            Self::Quad => 3,
        }
    }

    pub const fn bytes(self) -> u32 {
        1u32 << self.scale()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    Single,
    Double,
}

impl Precision {
    pub const fn from(bits: u8) -> Self {
        match bits {
            32 => Self::Single,
            _ => Self::Double,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Condition {
    Equal = 0x0,
    Distinct = 0x1,
    Carry = 0x2,
    Clear = 0x3,
    Minus = 0x4,
    Higher = 0x8,
    Lower = 0x9,
    Minimum = 0xA,
    Less = 0xB,
    Greater = 0xC,
    Maximum = 0xD,
}

#[derive(Clone, Copy)]
struct Patch {
    offset: usize,
    target: Label,
    narrow: bool,
}

#[derive(Default)]
pub struct Encoder {
    pub bytes: Vec<u8>,
    pub relocations: Vec<Relocation>,
    marks: Vec<Option<usize>>,
    patches: Vec<Patch>,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&mut self, word: u32) {
        self.bytes.extend_from_slice(&word.to_le_bytes());
    }

    pub fn fresh(&mut self) -> Label {
        let index = self.marks.len() as u32;
        self.marks.push(None);
        Label(index)
    }

    pub fn bind(&mut self, label: Label) {
        self.marks[label.0 as usize] = Some(self.bytes.len());
    }

    pub fn finish(mut self) -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
        let patches = std::mem::take(&mut self.patches);
        for item in patches {
            self.resolve(item)?;
        }
        Ok((self.bytes, self.relocations))
    }

    fn resolve(&mut self, item: Patch) -> Result<(), Fault> {
        let Some(destination) = self.marks[item.target.0 as usize] else {
            return Err(Fault::Unsupported(String::from("unbound arm branch label")));
        };
        let delta = (destination as i64 - item.offset as i64) >> 2;
        let slice = &mut self.bytes[item.offset..item.offset + 4];
        let base = u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]);
        let updated = Self::encode(base, delta, item.narrow)?;
        slice.copy_from_slice(&updated.to_le_bytes());
        Ok(())
    }

    fn encode(base: u32, delta: i64, narrow: bool) -> Result<u32, Fault> {
        if narrow {
            if !(-0x40000..=0x3FFFF).contains(&delta) {
                return Err(Fault::Unsupported(String::from("arm branch out of range")));
            }
            return Ok(base | (((delta as u32) & 0x7FFFF) << 5));
        }
        if !(-0x2000000..=0x1FFFFFF).contains(&delta) {
            return Err(Fault::Unsupported(String::from("arm jump out of range")));
        }
        Ok(base | ((delta as u32) & 0x03FF_FFFF))
    }

    pub fn constant(&mut self, target: u8, value: u64) {
        let parts = [
            (value & 0xFFFF) as u32,
            ((value >> 16) & 0xFFFF) as u32,
            ((value >> 32) & 0xFFFF) as u32,
            ((value >> 48) & 0xFFFF) as u32,
        ];
        let ones = parts.iter().filter(|&&part| part == 0xFFFF).count();
        if ones >= 2 && value != 0 {
            self.inverted(target, &parts);
            return;
        }
        let first = parts.iter().position(|&p| p != 0).unwrap_or(0);
        self.emit(0xD280_0000 | ((first as u32) << 21) | (parts[first] << 5) | u32::from(target));
        for (index, &part) in parts.iter().enumerate() {
            if index != first && part != 0 {
                self.emit(0xF280_0000 | ((index as u32) << 21) | (part << 5) | u32::from(target));
            }
        }
    }

    fn inverted(&mut self, target: u8, parts: &[u32; 4]) {
        let first = parts.iter().position(|&p| p != 0xFFFF).unwrap_or(0);
        let negated = (!parts[first]) & 0xFFFF;
        self.emit(0x9280_0000 | ((first as u32) << 21) | (negated << 5) | u32::from(target));
        for (index, &part) in parts.iter().enumerate() {
            if index != first && part != 0xFFFF {
                self.emit(0xF280_0000 | ((index as u32) << 21) | (part << 5) | u32::from(target));
            }
        }
    }

    pub fn jump(&mut self, target: Label) {
        let offset = self.bytes.len();
        self.emit(0x1400_0000);
        self.patches.push(Patch {
            offset,
            target,
            narrow: false,
        });
    }

    pub fn branch(&mut self, condition: Condition, target: Label) {
        let offset = self.bytes.len();
        self.emit(0x5400_0000 | (condition as u32));
        self.patches.push(Patch {
            offset,
            target,
            narrow: true,
        });
    }

    pub fn zero(&mut self, source: u8, nonzero: bool, target: Label) {
        let offset = self.bytes.len();
        let base = match nonzero {
            true => 0xB500_0000,
            false => 0xB400_0000,
        };
        self.emit(base | u32::from(source));
        self.patches.push(Patch {
            offset,
            target,
            narrow: true,
        });
    }

    pub fn call(&mut self, symbol: &str) {
        self.relocations.push(Relocation {
            offset: self.bytes.len() as u64,
            symbol: symbol.to_owned(),
            kind: Fixup::Call,
            addend: 0,
        });
        self.emit(0x9400_0000);
    }

    pub fn indirect(&mut self, target: u8) {
        self.emit(0xD63F_0000 | (u32::from(target) << 5));
    }

    pub fn address(&mut self, target: u8, symbol: &str, imported: bool) {
        self.relocations.push(Relocation {
            offset: self.bytes.len() as u64,
            symbol: symbol.to_owned(),
            kind: Fixup::Page,
            addend: 0,
        });
        self.emit(0x9000_0000 | u32::from(target));
        let scale = match imported {
            true => 3,
            false => 0,
        };
        self.relocations.push(Relocation {
            offset: self.bytes.len() as u64,
            symbol: symbol.to_owned(),
            kind: Fixup::Low { scale },
            addend: 0,
        });
        let second = match imported {
            true => 0xF940_0000,
            false => 0x9100_0000,
        };
        self.emit(second | (u32::from(target) << 5) | u32::from(target));
    }

    pub fn leave(&mut self) {
        self.emit(0xD65F_03C0);
    }

    pub fn trap(&mut self) {
        self.emit(0xD420_0020);
    }
}
