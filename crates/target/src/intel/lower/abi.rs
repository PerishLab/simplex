use super::Emitter;
use super::shape::Value;
use crate::intel::{Address, Flavor, Precision, Register, Width};
use crate::machine::Location;
use simplex_ir::{Function, Type};

#[derive(Clone)]
pub(crate) enum Callee {
    Direct(String),
    Indirect(u32),
}

pub(crate) fn indirect(flavor: Flavor, output: &Type) -> bool {
    match (flavor, output) {
        (Flavor::Linux, Type::Aggregate { size, .. }) => *size > 16,
        (Flavor::Windows, Type::Pair) => true,
        (Flavor::Windows, Type::Aggregate { size, .. }) => !matches!(*size, 0 | 1 | 2 | 4 | 8),
        _ => false,
    }
}

impl Emitter<'_> {
    pub(crate) fn prologue(&mut self, func: &Function) {
        self.encoder.push(Register::Rbp);
        self.encoder.copy(Register::Rbp, Register::Rsp);
        for &reg in &self.frame.used {
            self.encoder.push(Register::decode(reg));
        }
        self.encoder.adjust(Register::Rsp, -self.frame.reserve);
        match self.frame.flavor {
            Flavor::Linux => self.linux(func),
            Flavor::Windows => self.windows(func),
        }
    }

    pub(crate) fn epilogue(&mut self) {
        self.encoder.adjust(Register::Rsp, self.frame.reserve);
        for &reg in self.frame.used.iter().rev() {
            self.encoder.pop(Register::decode(reg));
        }
        self.encoder.pop(Register::Rbp);
        self.encoder.leave();
    }

    fn windows(&mut self, func: &Function) {
        let mut floats = [None; 4];
        let mut idx = usize::from(self.frame.hidden.is_some());
        for (_, kind) in &func.inputs {
            if matches!(kind, Type::Void | Type::Aggregate { size: 0, .. }) {
                continue;
            }
            if idx < 4
                && let Type::Float { bits } = kind
            {
                floats[idx] = Some(Precision::bits(*bits));
            }
            idx += 1;
        }
        let regs = Register::windows();
        for i in 0..4 {
            let home = Address::new(Register::Rbp, 16 + (i * 8) as i32);
            match floats[i] {
                Some(prec) => self.encoder.deposit(home, i as u8, prec),
                None => self.encoder.store(home, regs[i], Width::Quad),
            }
        }
        let mut slot = 0usize;
        if let Some(h) = self.frame.hidden {
            self.encoder
                .load(Register::Rax, Address::new(Register::Rbp, 16), Width::Quad);
            self.write(h, Register::Rax);
            slot = 1;
        }
        for &(id, ref kind) in &func.inputs {
            if matches!(kind, Type::Void | Type::Aggregate { size: 0, .. }) {
                continue;
            }
            let home = Address::new(Register::Rbp, 16 + (slot * 8) as i32);
            self.homed(id, kind, home);
            slot += 1;
        }
    }

    fn homed(&mut self, id: u32, kind: &Type, home: Address) {
        match (self.frame.value(id), kind) {
            (Value::General(dst), _) => {
                self.encoder.load(Register::Rax, home, Width::Quad);
                self.normalize(Register::Rax, kind);
                self.write(dst, Register::Rax);
            }
            (Value::Float { reg, prec }, _) => {
                self.encoder.fetch(0, home, prec);
                self.deposit(reg, 0);
            }
            (Value::Aggregate { slot, size }, _) if matches!(size, 1 | 2 | 4 | 8) => {
                let width = Width::bytes(size);
                self.encoder.load(Register::Rax, home, width);
                let dst = Address::new(Register::Rbp, self.frame.base(slot));
                self.encoder.store(dst, Register::Rax, width);
            }
            (Value::Pair { low, high }, _) => {
                self.encoder.load(Register::R10, home, Width::Quad);
                self.encoder
                    .load(Register::Rax, Address::new(Register::R10, 0), Width::Quad);
                self.write(low, Register::Rax);
                self.encoder
                    .load(Register::Rax, Address::new(Register::R10, 8), Width::Quad);
                self.write(high, Register::Rax);
            }
            (Value::Aggregate { slot, size }, _) => {
                self.encoder.load(Register::R11, home, Width::Quad);
                let dst = Address::new(Register::Rbp, self.frame.base(slot));
                self.encoder.address(Register::R10, dst);
                self.encoder.replicate(Register::R10, Register::R11, size);
            }
            _ => {}
        }
    }

    fn linux(&mut self, func: &Function) {
        let args = Register::linux();
        let mut general = Vec::new();
        let mut media = Vec::new();
        let mut deferred = Vec::new();
        let mut casts = Vec::new();
        let mut gp = 0usize;
        let mut fp = 0usize;
        let mut off = 0i32;
        if let Some(h) = self.frame.hidden {
            gp = 1;
            general.push((self.frame.location(h), Location::General(args[0].code())));
        }
        for &(id, ref kind) in &func.inputs {
            match (self.frame.value(id), kind) {
                (Value::Empty, _) => {}
                (Value::Float { reg, .. }, _) if fp < 8 => {
                    media.push((self.frame.location(reg), Location::Float(fp as u8)));
                    fp += 1;
                }
                (Value::Float { .. }, _) => {
                    deferred.push((id, kind.clone(), off));
                    off += 8;
                }
                (Value::Pair { low, high }, _) if gp + 2 <= 6 => {
                    general.push((self.frame.location(low), Location::General(args[gp].code())));
                    general.push((
                        self.frame.location(high),
                        Location::General(args[gp + 1].code()),
                    ));
                    gp += 2;
                }
                (Value::Aggregate { slot, size }, _) if size <= 8 && gp < 6 => {
                    let dst = Address::new(Register::Rbp, self.frame.base(slot));
                    self.encoder.unpack(dst, args[gp], size);
                    gp += 1;
                }
                (Value::Aggregate { slot, size }, _) if size <= 16 && gp + 2 <= 6 => {
                    let base = self.frame.base(slot);
                    self.encoder
                        .store(Address::new(Register::Rbp, base), args[gp], Width::Quad);
                    self.encoder.unpack(
                        Address::new(Register::Rbp, base + 8),
                        args[gp + 1],
                        size - 8,
                    );
                    gp += 2;
                }
                (Value::General(dst), _) if gp < 6 => {
                    general.push((self.frame.location(dst), Location::General(args[gp].code())));
                    casts.push((dst, kind.clone()));
                    gp += 1;
                }
                (_, Type::Pair) | (_, Type::Aggregate { size: 9..=16, .. }) => {
                    deferred.push((id, kind.clone(), off));
                    off += 16;
                }
                (
                    _,
                    Type::Aggregate {
                        size: s @ 17..,
                        align,
                        ..
                    },
                ) => {
                    let mask = (*align).max(8) as i32 - 1;
                    off = (off + mask) & !mask;
                    deferred.push((id, kind.clone(), off));
                    off += ((*s + 7) & !7) as i32;
                }
                _ => {
                    deferred.push((id, kind.clone(), off));
                    off += 8;
                }
            }
        }
        self.shuffle(&mut general);
        self.rearrange(&mut media);
        for (dst, kind) in casts {
            self.read(Register::Rax, dst);
            self.normalize(Register::Rax, &kind);
            self.write(dst, Register::Rax);
        }
        for (id, kind, disp) in deferred {
            self.stacked(id, &kind, 16 + disp);
        }
    }

    fn stacked(&mut self, id: u32, kind: &Type, disp: i32) {
        let src = Address::new(Register::Rbp, disp);
        match self.frame.value(id) {
            Value::General(dst) => {
                self.encoder.load(Register::Rax, src, Width::Quad);
                self.normalize(Register::Rax, kind);
                self.write(dst, Register::Rax);
            }
            Value::Float { reg, prec } => {
                self.encoder.fetch(0, src, prec);
                self.deposit(reg, 0);
            }
            Value::Pair { low, high } => {
                self.encoder.load(Register::Rax, src, Width::Quad);
                self.write(low, Register::Rax);
                self.encoder.load(
                    Register::Rax,
                    Address::new(Register::Rbp, disp + 8),
                    Width::Quad,
                );
                self.write(high, Register::Rax);
            }
            Value::Aggregate { slot, size } => {
                let dst = Address::new(Register::Rbp, self.frame.base(slot));
                self.encoder.address(Register::R10, dst);
                self.encoder.address(Register::R11, src);
                self.encoder.replicate(Register::R10, Register::R11, size);
            }
            Value::Empty => {}
        }
    }
}
