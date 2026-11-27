use super::{Encoder, Precision};

impl Encoder {
    pub fn ingest(&mut self, target: u8, source: u8, precision: Precision) {
        let opcode = match precision {
            Precision::Single => 0x1E27_0000,
            Precision::Double => 0x9E67_0000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn extract(&mut self, target: u8, source: u8, precision: Precision) {
        let opcode = match precision {
            Precision::Single => 0x1E26_0000,
            Precision::Double => 0x9E66_0000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn duplicate(&mut self, target: u8, source: u8, precision: Precision) {
        if target == source {
            return;
        }
        let opcode = match precision {
            Precision::Single => 0x1E20_4000,
            Precision::Double => 0x1E60_4000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn opposite(&mut self, target: u8, source: u8, precision: Precision) {
        let opcode = match precision {
            Precision::Single => 0x1E21_4000,
            Precision::Double => 0x1E61_4000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn arithmetic(&mut self, registers: (u8, u8, u8), operation: (u8, Precision)) {
        let (target, left, right) = registers;
        let base = match operation.1 {
            Precision::Single => 0x1E20_0800,
            Precision::Double => 0x1E60_0800,
        };
        let kind = match operation.0 {
            0 => 0x2000,
            1 => 0x3000,
            2 => 0x0000,
            _ => 0x1000,
        };
        self.emit(
            base | kind | (u32::from(right) << 16) | (u32::from(left) << 5) | u32::from(target),
        );
    }

    pub fn modulo(&mut self, registers: (u8, u8, u8), precision: Precision) {
        let (target, left, right) = registers;
        self.arithmetic((17, left, right), (3, precision));
        let round = match precision {
            Precision::Single => 0x1E25_C000,
            Precision::Double => 0x1E65_C000,
        };
        self.emit(round | (17 << 5) | 17);
        let fuse = match precision {
            Precision::Single => 0x1F00_8000,
            Precision::Double => 0x1F40_8000,
        };
        self.emit(
            fuse | (u32::from(right) << 16)
                | (u32::from(left) << 10)
                | (17 << 5)
                | u32::from(target),
        );
    }

    pub fn order(&mut self, left: u8, right: u8, precision: Precision) {
        let base = match precision {
            Precision::Single => 0x1E20_2000,
            Precision::Double => 0x1E60_2000,
        };
        self.emit(base | (u32::from(right) << 16) | (u32::from(left) << 5));
    }

    pub fn promote(&mut self, target: u8, source: u8, precision: Precision) {
        let opcode = match precision {
            Precision::Single => 0x9E22_0000,
            Precision::Double => 0x9E62_0000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn elevate(&mut self, target: u8, source: u8, precision: Precision) {
        let opcode = match precision {
            Precision::Single => 0x9E23_0000,
            Precision::Double => 0x9E63_0000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn truncate(&mut self, target: u8, source: u8, precision: Precision) {
        let opcode = match precision {
            Precision::Single => 0x9E38_0000,
            Precision::Double => 0x9E78_0000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn demote(&mut self, target: u8, source: u8, precision: Precision) {
        let opcode = match precision {
            Precision::Single => 0x9E39_0000,
            Precision::Double => 0x9E79_0000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }

    pub fn convert(&mut self, target: u8, source: u8, widen: bool) {
        let opcode = match widen {
            true => 0x1E22_C000,
            false => 0x1E62_4000,
        };
        self.emit(opcode | (u32::from(source) << 5) | u32::from(target));
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Condition, Encoder, Precision, Width};

    #[test]
    fn opcodes() {
        let mut enc = Encoder::new();
        enc.constant(0, 0x1234_0000);
        enc.constant(1, (-4661i64) as u64);
        enc.add(0, 1, 2);
        enc.sub(0, 1, 2);
        enc.multiply(0, 1, 2);
        enc.divide((0, 1, 2), true);
        enc.divide((0, 1, 2), false);
        enc.remainder((0, 1, 2), true);
        enc.bitand(0, 1, 2);
        enc.bitor(0, 1, 2);
        enc.bitxor(0, 1, 2);
        enc.shift((0, 1, 2), (0, true));
        enc.shift((0, 1, 2), (1, true));
        enc.shift((0, 1, 2), (2, true));
        enc.compare(1, 2);
        enc.flag(0, Condition::Equal);
        enc.load(0, (1, 1), (Width::Byte, true));
        enc.load(0, (1, 2), (Width::Half, true));
        enc.load(0, (1, 4), (Width::Word, true));
        enc.stash((19, 20), 16, false);
        enc.restore((19, 20), 16, false);
        enc.stash((8, 9), 16, true);
        enc.restore((8, 9), 16, true);
        enc.ingest(0, 1, Precision::Double);
        enc.arithmetic((0, 1, 2), (0, Precision::Double));
        enc.arithmetic((0, 1, 2), (1, Precision::Double));
        enc.arithmetic((0, 1, 2), (2, Precision::Double));
        enc.arithmetic((0, 1, 2), (3, Precision::Double));
        enc.order(0, 1, Precision::Double);
        enc.promote(0, 1, Precision::Double);
        enc.elevate(0, 1, Precision::Double);
        enc.truncate(0, 1, Precision::Double);
        enc.demote(0, 1, Precision::Double);
        enc.convert(0, 1, true);
        enc.convert(0, 1, false);
        let (raw, _) = enc.finish().expect("opcodes");
        let words: Vec<u32> = raw
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(words[0..2], [0xD2A2_4680, 0x9282_4681]);
        assert!(words.contains(&0x9E67_0020));
        assert!(words.contains(&0x1E22_C020));
        assert!(words.contains(&0x1E62_4020));
    }
}
