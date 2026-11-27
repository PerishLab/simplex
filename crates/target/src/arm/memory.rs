use super::{Condition, Encoder, Precision, Width};

impl Encoder {
    pub fn enter(&mut self) {
        self.emit(0xA9BF_7BFD);
        self.emit(0x9100_03FD);
    }

    pub fn exit(&mut self) {
        self.emit(0x9100_03BF);
        self.emit(0xA8C1_7BFD);
        self.leave();
    }

    pub fn displace(&mut self, target: u8, base: u8, delta: i64) {
        if delta == 0 {
            if target != base {
                match target == 31 || base == 31 {
                    true => self.emit(0x9100_0000 | (u32::from(base) << 5) | u32::from(target)),
                    false => self.copy(target, base),
                }
            }
            return;
        }
        if (1..=4095).contains(&delta) {
            self.emit(
                0x9100_0000 | ((delta as u32) << 10) | (u32::from(base) << 5) | u32::from(target),
            );
            return;
        }
        if (-4095..=-1).contains(&delta) {
            let magnitude = (-delta) as u32;
            self.emit(0xD100_0000 | (magnitude << 10) | (u32::from(base) << 5) | u32::from(target));
            return;
        }
        self.large(target, base, delta);
    }

    fn large(&mut self, target: u8, base: u8, delta: i64) {
        let magnitude = delta.unsigned_abs();
        if (magnitude & 0xFFF) == 0 && (magnitude >> 12) <= 4095 {
            let shifted = (magnitude >> 12) as u32;
            let opcode = match delta > 0 {
                true => 0x9140_0000,
                false => 0xD140_0000,
            };
            self.emit(opcode | (shifted << 10) | (u32::from(base) << 5) | u32::from(target));
            return;
        }
        let helper = match base == 16 {
            true => 17,
            false => 16,
        };
        self.constant(helper, magnitude);
        match (delta > 0, target == 31 || base == 31) {
            (true, true) => self.emit(
                0x8B20_6000
                    | (u32::from(helper) << 16)
                    | (u32::from(base) << 5)
                    | u32::from(target),
            ),
            (false, true) => self.emit(
                0xCB20_6000
                    | (u32::from(helper) << 16)
                    | (u32::from(base) << 5)
                    | u32::from(target),
            ),
            (true, false) => self.add(target, base, helper),
            (false, false) => self.sub(target, base, helper),
        }
    }

    fn effective(&mut self, base: u8, offset: i64, parameters: (u8, u8)) -> (u8, u32) {
        let (scale, avoid) = parameters;
        let unit = 1i64 << scale;
        if offset >= 0 && offset % unit == 0 && (offset / unit) <= 4095 {
            return (base, (offset / unit) as u32);
        }
        let helper = match avoid == 16 {
            true => 17,
            false => 16,
        };
        self.displace(helper, base, offset);
        (helper, 0)
    }

    pub fn load(&mut self, target: u8, location: (u8, i64), access: (Width, bool)) {
        let (width, signed) = access;
        let (base, index) = self.effective(location.0, location.1, (width.scale(), target));
        let opcode = match (width, signed) {
            (Width::Byte, false) => 0x3940_0000,
            (Width::Byte, true) => 0x3980_0000,
            (Width::Half, false) => 0x7940_0000,
            (Width::Half, true) => 0x7980_0000,
            (Width::Word, false) => 0xB940_0000,
            (Width::Word, true) => 0xB980_0000,
            (Width::Quad, _) => 0xF940_0000,
        };
        self.emit(opcode | (index << 10) | (u32::from(base) << 5) | u32::from(target));
    }

    pub fn store(&mut self, source: u8, location: (u8, i64), width: Width) {
        let (base, index) = self.effective(location.0, location.1, (width.scale(), source));
        let opcode = match width {
            Width::Byte => 0x3900_0000,
            Width::Half => 0x7900_0000,
            Width::Word => 0xB900_0000,
            Width::Quad => 0xF900_0000,
        };
        self.emit(opcode | (index << 10) | (u32::from(base) << 5) | u32::from(source));
    }

    pub fn sample(&mut self, target: u8, location: (u8, i64), precision: Precision) {
        let scale = match precision {
            Precision::Single => 2,
            Precision::Double => 3,
        };
        let (base, index) = self.effective(location.0, location.1, (scale, 31));
        let opcode = match precision {
            Precision::Single => 0xBD40_0000,
            Precision::Double => 0xFD40_0000,
        };
        self.emit(opcode | (index << 10) | (u32::from(base) << 5) | u32::from(target));
    }

    pub fn deposit(&mut self, source: u8, location: (u8, i64), precision: Precision) {
        let scale = match precision {
            Precision::Single => 2,
            Precision::Double => 3,
        };
        let (base, index) = self.effective(location.0, location.1, (scale, 31));
        let opcode = match precision {
            Precision::Single => 0xBD00_0000,
            Precision::Double => 0xFD00_0000,
        };
        self.emit(opcode | (index << 10) | (u32::from(base) << 5) | u32::from(source));
    }

    pub fn stash(&mut self, pair: (u8, u8), offset: u32, float: bool) {
        if offset.is_multiple_of(8) && (offset / 8) <= 63 {
            let opcode = match float {
                true => 0x6D00_0000,
                false => 0xA900_0000,
            };
            let scaled = (offset / 8) & 0x7F;
            self.emit(
                opcode | (scaled << 15) | (u32::from(pair.1) << 10) | (31 << 5) | u32::from(pair.0),
            );
            return;
        }
        match float {
            true => {
                self.deposit(pair.0, (31, i64::from(offset)), Precision::Double);
                self.deposit(pair.1, (31, i64::from(offset) + 8), Precision::Double);
            }
            false => {
                self.store(pair.0, (31, i64::from(offset)), Width::Quad);
                self.store(pair.1, (31, i64::from(offset) + 8), Width::Quad);
            }
        }
    }

    pub fn restore(&mut self, pair: (u8, u8), offset: u32, float: bool) {
        if offset.is_multiple_of(8) && (offset / 8) <= 63 {
            let opcode = match float {
                true => 0x6D40_0000,
                false => 0xA940_0000,
            };
            let scaled = (offset / 8) & 0x7F;
            self.emit(
                opcode | (scaled << 15) | (u32::from(pair.1) << 10) | (31 << 5) | u32::from(pair.0),
            );
            return;
        }
        match float {
            true => {
                self.sample(pair.0, (31, i64::from(offset)), Precision::Double);
                self.sample(pair.1, (31, i64::from(offset) + 8), Precision::Double);
            }
            false => {
                self.load(pair.0, (31, i64::from(offset)), (Width::Quad, false));
                self.load(pair.1, (31, i64::from(offset) + 8), (Width::Quad, false));
            }
        }
    }

    pub fn transfer(&mut self, destination: u8, source: u8, bytes: u32) {
        if bytes == 0 {
            return;
        }
        let mut cursor = 0i64;
        let mut remaining = bytes;
        if remaining > 64 {
            let quads = remaining / 8;
            remaining %= 8;
            self.constant(15, u64::from(quads));
            let header = self.fresh();
            self.bind(header);
            self.load(16, (source, 0), (Width::Quad, false));
            self.store(16, (destination, 0), Width::Quad);
            self.displace(source, source, 8);
            self.displace(destination, destination, 8);
            self.emit(0xF100_0400 | (15 << 5) | 15);
            self.branch(Condition::Distinct, header);
        }
        while remaining >= 8 {
            self.load(15, (source, cursor), (Width::Quad, false));
            self.store(15, (destination, cursor), Width::Quad);
            cursor += 8;
            remaining -= 8;
        }
        for step in [4u32, 2, 1] {
            if remaining >= step {
                let width = Width::from(step);
                self.load(15, (source, cursor), (width, false));
                self.store(15, (destination, cursor), width);
                cursor += i64::from(step);
                remaining -= step;
            }
        }
    }

    pub fn fence(&mut self) {
        self.emit(0xD503_3BBF);
    }

    pub fn acquire(&mut self, target: u8, base: u8, width: Width) {
        let scale = u32::from(width.scale());
        self.emit(0x08DF_FC00 | (scale << 30) | (u32::from(base) << 5) | u32::from(target));
    }

    pub fn release(&mut self, source: u8, base: u8, width: Width) {
        let scale = u32::from(width.scale());
        self.emit(0x089F_FC00 | (scale << 30) | (u32::from(base) << 5) | u32::from(source));
    }

    pub fn exclusive(&mut self, target: u8, base: u8, width: Width) {
        let scale = u32::from(width.scale());
        self.emit(0x085F_FC00 | (scale << 30) | (u32::from(base) << 5) | u32::from(target));
    }

    pub fn commit(&mut self, status: u8, access: (u8, u8), width: Width) {
        let scale = u32::from(width.scale());
        self.emit(
            0x0800_FC00
                | (scale << 30)
                | (u32::from(status) << 16)
                | (u32::from(access.1) << 5)
                | u32::from(access.0),
        );
    }
}
