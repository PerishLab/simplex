use super::call::{Cursor, align};
use super::{Binding, Context};
use crate::arm::{Precision, Width};
use simplex_ir::Type;

pub struct Call<'a> {
    pub arguments: &'a [u32],
    pub inputs: &'a [Type],
    pub output: &'a Type,
    pub destination: u32,
}

impl Context<'_> {
    pub fn invoke(&mut self, site: (usize, usize), callee: Result<&str, u32>, spec: Call<'_>) {
        let mut cursor = Cursor::default();
        for (index, (&arg, kind)) in spec.arguments.iter().zip(spec.inputs.iter()).enumerate() {
            self.pass((site.0, site.1, index), (arg, kind), &mut cursor);
        }
        if let Type::Aggregate { size, .. } = *spec.output
            && size > 16
            && let Binding::Slot(slot) = self.bindings[spec.destination as usize]
        {
            self.encoder
                .displace(8, 31, i64::from(self.locals[slot].offset));
        }
        match callee {
            Ok(name) => {
                let (mapped, _) = self.resolver.symbol(name);
                self.encoder.call(&mapped);
            }
            Err(target) => {
                let reg = match self.bindings[target as usize] {
                    Binding::Single(vid) => self.get(vid, 16),
                    _ => 16,
                };
                self.encoder.indirect(reg);
            }
        }
        self.capture(spec.destination, spec.output);
    }

    fn pass(&mut self, site: (usize, usize, usize), argument: (u32, &Type), cursor: &mut Cursor) {
        let (id, kind) = argument;
        let binding = self.bindings[id as usize];
        match (kind, binding) {
            (Type::Void, _) | (_, Binding::Empty) => {}
            (Type::Float { bits }, Binding::Single(vid)) => self.send(vid, *bits, cursor),
            (Type::Pair, Binding::Pair(low, high)) => self.couple((low, high), cursor),
            (Type::Aggregate { size, align: a, .. }, Binding::Slot(slot)) => {
                self.marshal(site, (*size, *a, slot), cursor);
            }
            (_, Binding::Single(vid)) => self.word(vid, kind, cursor),
            _ => {}
        }
    }

    fn send(&mut self, vid: u32, bits: u8, cursor: &mut Cursor) {
        let precision = Precision::from(bits);
        let val = self.fetch(vid, 16, precision);
        if cursor.media < 8 {
            let target = cursor.media as u8;
            cursor.media += 1;
            self.encoder.duplicate(target, val, precision);
            return;
        }
        let bytes = u32::from(bits / 8).max(4);
        cursor.stack = align(cursor.stack, bytes);
        self.encoder
            .deposit(val, (31, i64::from(cursor.stack)), precision);
        cursor.stack += bytes;
    }

    fn couple(&mut self, pair: (u32, u32), cursor: &mut Cursor) {
        if cursor.general + 2 <= 8 {
            let first = cursor.general as u8;
            cursor.general += 2;
            let lv = self.get(pair.0, 15);
            self.encoder.copy(first, lv);
            let hv = self.get(pair.1, 15);
            self.encoder.copy(first + 1, hv);
            return;
        }
        cursor.general = 8;
        cursor.stack = align(cursor.stack, 8);
        let lv = self.get(pair.0, 15);
        self.encoder
            .store(lv, (31, i64::from(cursor.stack)), Width::Quad);
        let hv = self.get(pair.1, 15);
        self.encoder
            .store(hv, (31, i64::from(cursor.stack) + 8), Width::Quad);
        cursor.stack += 16;
    }

    fn word(&mut self, vid: u32, kind: &Type, cursor: &mut Cursor) {
        let val = self.get(vid, 15);
        self.normalize(15, val, kind);
        if cursor.general < 8 {
            let target = cursor.general as u8;
            cursor.general += 1;
            self.encoder.copy(target, 15);
            return;
        }
        let bytes = match *kind {
            Type::Integer { bits, .. } => u32::from(bits.div_ceil(8)).max(1).next_power_of_two(),
            _ => 8,
        };
        cursor.stack = align(cursor.stack, bytes);
        self.encoder
            .store(15, (31, i64::from(cursor.stack)), Width::from(bytes));
        cursor.stack += bytes;
    }

    fn marshal(
        &mut self,
        site: (usize, usize, usize),
        spec: (u32, u32, usize),
        cursor: &mut Cursor,
    ) {
        let (size, alignment, slot) = spec;
        if size == 0 {
            return;
        }
        let origin = i64::from(self.locals[slot].offset);
        if size <= 8 {
            self.tiny((size, alignment, origin), cursor);
            return;
        }
        if size <= 16 {
            self.paired((size, alignment, origin), cursor);
            return;
        }
        self.pointed(site, (size, origin), cursor);
    }

    fn tiny(&mut self, spec: (u32, u32, i64), cursor: &mut Cursor) {
        let (size, alignment, origin) = spec;
        if cursor.general < 8 {
            let target = cursor.general as u8;
            cursor.general += 1;
            self.partial(target, (31, origin), size);
            return;
        }
        let step = alignment.clamp(1, 8);
        cursor.stack = align(cursor.stack, step);
        self.encoder.displace(14, 31, i64::from(cursor.stack));
        self.encoder.displace(16, 31, origin);
        self.encoder.transfer(14, 16, size);
        cursor.stack += (size + step - 1) & !(step - 1);
    }

    fn paired(&mut self, spec: (u32, u32, i64), cursor: &mut Cursor) {
        let (size, alignment, origin) = spec;
        if cursor.general + 2 <= 8 {
            let first = cursor.general as u8;
            cursor.general += 2;
            self.partial(first, (31, origin), 8);
            self.partial(first + 1, (31, origin + 8), size - 8);
            return;
        }
        cursor.general = 8;
        cursor.stack = align(cursor.stack, alignment.clamp(8, 16));
        self.encoder.displace(14, 31, i64::from(cursor.stack));
        self.encoder.displace(16, 31, origin);
        self.encoder.transfer(14, 16, size);
        cursor.stack += 16;
    }

    fn pointed(&mut self, site: (usize, usize, usize), spec: (u32, i64), cursor: &mut Cursor) {
        let (size, origin) = spec;
        let duplicate = self.copies[&site];
        let cloned = i64::from(self.locals[duplicate].offset);
        self.encoder.displace(14, 31, cloned);
        self.encoder.displace(16, 31, origin);
        self.encoder.transfer(14, 16, size);
        if cursor.general < 8 {
            let target = cursor.general as u8;
            cursor.general += 1;
            self.encoder.displace(target, 31, cloned);
            return;
        }
        cursor.stack = align(cursor.stack, 8);
        self.encoder.displace(15, 31, cloned);
        self.encoder
            .store(15, (31, i64::from(cursor.stack)), Width::Quad);
        cursor.stack += 8;
    }

    fn capture(&mut self, destination: u32, output: &Type) {
        let binding = self.bindings[destination as usize];
        match (output, binding) {
            (Type::Void, _) | (_, Binding::Empty) => {}
            (Type::Float { bits }, Binding::Single(vid)) => {
                self.place(vid, 0, Precision::from(*bits));
            }
            (Type::Pair, Binding::Pair(low, high)) => {
                self.encoder.copy(15, 1);
                self.put(low, 0);
                self.put(high, 15);
            }
            (Type::Aggregate { size, .. }, Binding::Slot(slot)) => {
                let offset = i64::from(self.locals[slot].offset);
                if *size > 0 && *size <= 8 {
                    self.scatter(0, (31, offset), *size);
                } else if *size > 8 && *size <= 16 {
                    self.scatter(0, (31, offset), 8);
                    self.scatter(1, (31, offset + 8), *size - 8);
                }
            }
            (_, Binding::Single(vid)) => {
                self.normalize(15, 0, output);
                self.put(vid, 15);
            }
            _ => {}
        }
    }

    pub fn partial(&mut self, target: u8, location: (u8, i64), bytes: u32) {
        match bytes {
            8 => self.encoder.load(target, location, (Width::Quad, false)),
            4 => self.encoder.load(target, location, (Width::Word, false)),
            2 => self.encoder.load(target, location, (Width::Half, false)),
            1 => self.encoder.load(target, location, (Width::Byte, false)),
            _ => self.gather(target, location, bytes),
        }
    }

    fn gather(&mut self, target: u8, location: (u8, i64), bytes: u32) {
        self.encoder.constant(target, 0);
        let mut cursor = 0u32;
        let mut rest = bytes;
        for step in [4u32, 2, 1] {
            if rest < step {
                continue;
            }
            self.encoder.load(
                15,
                (location.0, location.1 + i64::from(cursor)),
                (Width::from(step), false),
            );
            if cursor > 0 {
                self.encoder.constant(16, u64::from(cursor * 8));
                self.encoder.shift((15, 15, 16), (0, true));
            }
            self.encoder.bitor(target, target, 15);
            cursor += step;
            rest -= step;
        }
    }

    pub fn scatter(&mut self, source: u8, location: (u8, i64), bytes: u32) {
        match bytes {
            8 => self.encoder.store(source, location, Width::Quad),
            4 => self.encoder.store(source, location, Width::Word),
            2 => self.encoder.store(source, location, Width::Half),
            1 => self.encoder.store(source, location, Width::Byte),
            _ => self.spread(source, location, bytes),
        }
    }

    fn spread(&mut self, source: u8, location: (u8, i64), bytes: u32) {
        self.encoder.copy(14, source);
        let mut cursor = 0u32;
        let mut rest = bytes;
        for step in [4u32, 2, 1] {
            if rest < step {
                continue;
            }
            self.encoder.store(
                14,
                (location.0, location.1 + i64::from(cursor)),
                Width::from(step),
            );
            cursor += step;
            rest -= step;
            if rest > 0 {
                self.encoder.constant(16, u64::from(step * 8));
                self.encoder.shift((14, 14, 16), (1, true));
            }
        }
    }
}
