use super::{Binding, Context};
use crate::arm::{Precision, Width};
use simplex_ir::Type;

#[derive(Default)]
pub struct Cursor {
    pub general: usize,
    pub media: usize,
    pub stack: u32,
}

pub const fn align(value: u32, step: u32) -> u32 {
    let mask = step - 1;
    (value + mask) & !mask
}

pub fn footprint(inputs: &[Type]) -> u32 {
    let mut cursor = Cursor::default();
    for kind in inputs {
        cursor.advance(kind);
    }
    (cursor.stack + 15) & !15
}

impl Cursor {
    fn advance(&mut self, kind: &Type) {
        match *kind {
            Type::Void | Type::Aggregate { size: 0, .. } => {}
            Type::Float { bits } => self.media(bits),
            Type::Pair => self.pair(8),
            Type::Aggregate { size, align: a, .. } if size <= 8 => self.compact(size, a),
            Type::Aggregate { size, align: a, .. } if size <= 16 => {
                let _ = size;
                self.pair(a.clamp(8, 16));
            }
            Type::Aggregate { .. } | Type::Pointer => self.word(8),
            Type::Integer { bits, .. } => {
                let bytes = u32::from(bits.div_ceil(8)).max(1).next_power_of_two();
                self.word(bytes);
            }
        }
    }

    fn media(&mut self, bits: u8) {
        if self.media < 8 {
            self.media += 1;
            return;
        }
        let bytes = u32::from(bits / 8).max(4);
        self.stack = align(self.stack, bytes) + bytes;
    }

    fn pair(&mut self, step: u32) {
        if self.general + 2 <= 8 {
            self.general += 2;
            return;
        }
        self.general = 8;
        self.stack = align(self.stack, step) + 16;
    }

    fn compact(&mut self, size: u32, alignment: u32) {
        if self.general < 8 {
            self.general += 1;
            return;
        }
        let step = alignment.clamp(1, 8);
        self.stack = align(self.stack, step) + ((size + step - 1) & !(step - 1));
    }

    fn word(&mut self, bytes: u32) {
        if self.general < 8 {
            self.general += 1;
            return;
        }
        self.stack = align(self.stack, bytes) + bytes;
    }
}

impl Context<'_> {
    pub fn prologue(&mut self) {
        self.encoder.enter();
        if self.total > 0 {
            self.encoder.displace(31, 31, -i64::from(self.total));
        }
        self.preserve(false);
        self.preserve(true);
        if let Some(vid) = self.indirect {
            self.put(vid, 8);
        }
        let mut cursor = Cursor::default();
        for index in 0..self.function.inputs.len() {
            let (id, ref kind) = self.function.inputs[index];
            let cloned = kind.clone();
            self.accept(id, &cloned, &mut cursor);
        }
    }

    pub fn epilogue(&mut self) {
        self.recover(true);
        self.recover(false);
        self.encoder.exit();
    }

    fn preserve(&mut self, float: bool) {
        let (base, list) = match float {
            true => (self.float.0, self.float.1.clone()),
            false => (self.general.0, self.general.1.clone()),
        };
        let mut index = 0usize;
        while index + 1 < list.len() {
            let offset = base + (index * 8) as u32;
            self.encoder
                .stash((list[index], list[index + 1]), offset, float);
            index += 2;
        }
        if index < list.len() {
            let offset = i64::from(base + (index * 8) as u32);
            match float {
                true => self
                    .encoder
                    .deposit(list[index], (31, offset), Precision::Double),
                false => self.encoder.store(list[index], (31, offset), Width::Quad),
            }
        }
    }

    fn recover(&mut self, float: bool) {
        let (base, list) = match float {
            true => (self.float.0, self.float.1.clone()),
            false => (self.general.0, self.general.1.clone()),
        };
        let mut index = 0usize;
        while index + 1 < list.len() {
            let offset = base + (index * 8) as u32;
            self.encoder
                .restore((list[index], list[index + 1]), offset, float);
            index += 2;
        }
        if index < list.len() {
            let offset = i64::from(base + (index * 8) as u32);
            match float {
                true => self
                    .encoder
                    .sample(list[index], (31, offset), Precision::Double),
                false => self
                    .encoder
                    .load(list[index], (31, offset), (Width::Quad, false)),
            }
        }
    }

    fn accept(&mut self, id: u32, kind: &Type, cursor: &mut Cursor) {
        let binding = self.bindings[id as usize];
        match (kind, binding) {
            (Type::Void, _) | (_, Binding::Empty) => {}
            (Type::Float { bits }, Binding::Single(vid)) => self.floating(vid, *bits, cursor),
            (Type::Pair, Binding::Pair(low, high)) => self.coupled((low, high), cursor),
            (Type::Aggregate { size, align: a, .. }, Binding::Slot(slot)) => {
                self.composite((*size, *a, slot), cursor);
            }
            (_, Binding::Single(vid)) => self.scalar(vid, kind, cursor),
            _ => {}
        }
    }

    fn floating(&mut self, vid: u32, bits: u8, cursor: &mut Cursor) {
        let precision = Precision::from(bits);
        if cursor.media < 8 {
            let source = cursor.media as u8;
            cursor.media += 1;
            self.place(vid, source, precision);
            return;
        }
        let bytes = u32::from(bits / 8).max(4);
        cursor.stack = align(cursor.stack, bytes);
        self.encoder
            .sample(16, (29, 16 + i64::from(cursor.stack)), precision);
        cursor.stack += bytes;
        self.place(vid, 16, precision);
    }

    fn coupled(&mut self, pair: (u32, u32), cursor: &mut Cursor) {
        if cursor.general + 2 <= 8 {
            let first = cursor.general as u8;
            cursor.general += 2;
            self.put(pair.0, first);
            self.put(pair.1, first + 1);
            return;
        }
        cursor.general = 8;
        cursor.stack = align(cursor.stack, 8);
        self.encoder
            .load(15, (29, 16 + i64::from(cursor.stack)), (Width::Quad, false));
        self.put(pair.0, 15);
        self.encoder
            .load(15, (29, 24 + i64::from(cursor.stack)), (Width::Quad, false));
        self.put(pair.1, 15);
        cursor.stack += 16;
    }

    fn scalar(&mut self, vid: u32, kind: &Type, cursor: &mut Cursor) {
        if cursor.general < 8 {
            let reg = cursor.general as u8;
            cursor.general += 1;
            self.normalize(15, reg, kind);
            self.put(vid, 15);
            return;
        }
        let (bytes, signed) = match *kind {
            Type::Integer { bits, signed } => (
                u32::from(bits.div_ceil(8)).max(1).next_power_of_two(),
                signed,
            ),
            _ => (8, false),
        };
        cursor.stack = align(cursor.stack, bytes);
        self.encoder.load(
            15,
            (29, 16 + i64::from(cursor.stack)),
            (Width::from(bytes), signed),
        );
        cursor.stack += bytes;
        self.normalize(15, 15, kind);
        self.put(vid, 15);
    }

    fn composite(&mut self, spec: (u32, u32, usize), cursor: &mut Cursor) {
        let (size, alignment, slot) = spec;
        if size == 0 {
            return;
        }
        let target = i64::from(self.locals[slot].offset);
        if size <= 8 {
            self.small((size, alignment, target), cursor);
            return;
        }
        if size <= 16 {
            self.double((size, alignment, target), cursor);
            return;
        }
        self.pointer((size, target), cursor);
    }

    fn small(&mut self, spec: (u32, u32, i64), cursor: &mut Cursor) {
        let (size, alignment, target) = spec;
        if cursor.general < 8 {
            let reg = cursor.general as u8;
            cursor.general += 1;
            self.encoder.store(reg, (31, target), Width::Quad);
            return;
        }
        let step = alignment.clamp(1, 8);
        cursor.stack = align(cursor.stack, step);
        self.encoder.displace(14, 31, target);
        self.encoder.displace(16, 29, 16 + i64::from(cursor.stack));
        self.encoder.transfer(14, 16, size);
        cursor.stack += (size + step - 1) & !(step - 1);
    }

    fn double(&mut self, spec: (u32, u32, i64), cursor: &mut Cursor) {
        let (size, alignment, target) = spec;
        if cursor.general + 2 <= 8 {
            let first = cursor.general as u8;
            cursor.general += 2;
            self.encoder.store(first, (31, target), Width::Quad);
            self.encoder.store(first + 1, (31, target + 8), Width::Quad);
            return;
        }
        cursor.general = 8;
        cursor.stack = align(cursor.stack, alignment.clamp(8, 16));
        self.encoder.displace(14, 31, target);
        self.encoder.displace(16, 29, 16 + i64::from(cursor.stack));
        self.encoder.transfer(14, 16, size);
        cursor.stack += 16;
    }

    fn pointer(&mut self, spec: (u32, i64), cursor: &mut Cursor) {
        let (size, target) = spec;
        if cursor.general < 8 {
            let reg = cursor.general as u8;
            cursor.general += 1;
            self.encoder.copy(16, reg);
        } else {
            cursor.stack = align(cursor.stack, 8);
            self.encoder
                .load(16, (29, 16 + i64::from(cursor.stack)), (Width::Quad, false));
            cursor.stack += 8;
        }
        self.encoder.displace(14, 31, target);
        self.encoder.transfer(14, 16, size);
    }
}
