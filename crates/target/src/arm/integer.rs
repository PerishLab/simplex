use super::{Condition, Encoder};

impl Encoder {
    pub fn copy(&mut self, target: u8, source: u8) {
        if target != source {
            self.emit(0xAA00_03E0 | (u32::from(source) << 16) | u32::from(target));
        }
    }

    pub fn narrow(&mut self, target: u8, source: u8) {
        self.emit(0x2A00_03E0 | (u32::from(source) << 16) | u32::from(target));
    }

    pub fn extend(&mut self, target: u8, source: u8, bits: u8) {
        match bits {
            1 => self.emit(0x9240_0000 | (u32::from(source) << 5) | u32::from(target)),
            8 => self.emit(0x5300_1C00 | (u32::from(source) << 5) | u32::from(target)),
            16 => self.emit(0x5300_3C00 | (u32::from(source) << 5) | u32::from(target)),
            32 => self.narrow(target, source),
            _ => self.copy(target, source),
        }
    }

    pub fn widen(&mut self, target: u8, source: u8, bits: u8) {
        match bits {
            1 => self.emit(0x9240_0000 | (u32::from(source) << 5) | u32::from(target)),
            8 => self.emit(0x9340_1C00 | (u32::from(source) << 5) | u32::from(target)),
            16 => self.emit(0x9340_3C00 | (u32::from(source) << 5) | u32::from(target)),
            32 => self.emit(0x9340_7C00 | (u32::from(source) << 5) | u32::from(target)),
            _ => self.copy(target, source),
        }
    }

    pub fn add(&mut self, target: u8, left: u8, right: u8) {
        self.emit(
            0x8B00_0000 | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target),
        );
    }

    pub fn sub(&mut self, target: u8, left: u8, right: u8) {
        self.emit(
            0xCB00_0000 | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target),
        );
    }

    pub fn negate(&mut self, target: u8, source: u8) {
        self.sub(target, 31, source);
    }

    pub fn multiply(&mut self, target: u8, left: u8, right: u8) {
        self.emit(
            0x9B00_7C00 | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target),
        );
    }

    pub fn divide(&mut self, registers: (u8, u8, u8), signed: bool) {
        let (target, left, right) = registers;
        let base = match signed {
            true => 0x9AC0_0C00,
            false => 0x9AC0_0800,
        };
        self.emit(base | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target));
    }

    pub fn remainder(&mut self, registers: (u8, u8, u8), signed: bool) {
        let (target, left, right) = registers;
        let quotient = [15u8, 16, 17]
            .into_iter()
            .find(|&candidate| candidate != left && candidate != right)
            .unwrap_or(16);
        self.divide((quotient, left, right), signed);
        self.emit(
            0x9B00_8000
                | (u32::from(right) << 16)
                | (u32::from(left) << 10)
                | (u32::from(quotient) << 5)
                | u32::from(target),
        );
    }

    pub fn bitand(&mut self, target: u8, left: u8, right: u8) {
        self.emit(
            0x8A00_0000 | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target),
        );
    }

    pub fn bitor(&mut self, target: u8, left: u8, right: u8) {
        self.emit(
            0xAA00_0000 | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target),
        );
    }

    pub fn bitxor(&mut self, target: u8, left: u8, right: u8) {
        self.emit(
            0xCA00_0000 | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target),
        );
    }

    pub fn invert(&mut self, target: u8, source: u8) {
        self.emit(0xAA20_03E0 | (u32::from(source) << 16) | u32::from(target));
    }

    pub fn flip(&mut self, target: u8, source: u8) {
        self.emit(0xD240_0000 | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn shift(&mut self, registers: (u8, u8, u8), mode: (u8, bool)) {
        let (target, left, right) = registers;
        let base = match mode.1 {
            true => 0x9AC0_0000,
            false => 0x1AC0_0000,
        };
        let kind = match mode.0 {
            0 => 0x2000,
            1 => 0x2400,
            _ => 0x2800,
        };
        self.emit(
            base | kind | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target),
        );
    }

    pub fn compare(&mut self, left: u8, right: u8) {
        self.emit(0xEB00_001F | (u32::from(right) << 16) | (u32::from(left) << 5));
    }

    pub fn flag(&mut self, target: u8, condition: Condition) {
        let code = (condition as u32) ^ 1;
        self.emit(0x9A9F_07E0 | (code << 12) | u32::from(target));
    }
}
