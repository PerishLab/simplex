use super::{Binding, Context};
use crate::arm::{Condition, Precision};
use simplex_ir::{Binary, Cast, Type, Unary};

const fn relation(op: Binary, signed: bool) -> Condition {
    match (op, signed) {
        (Binary::Eq, _) => Condition::Equal,
        (Binary::Ne, _) => Condition::Distinct,
        (Binary::Lt, true) => Condition::Less,
        (Binary::Lt, false) => Condition::Clear,
        (Binary::Le, true) => Condition::Maximum,
        (Binary::Le, false) => Condition::Lower,
        (Binary::Gt, true) => Condition::Greater,
        (Binary::Gt, false) => Condition::Higher,
        (_, true) => Condition::Minimum,
        (_, false) => Condition::Carry,
    }
}

impl Context<'_> {
    pub fn pair(&mut self, low: u32, high: u32, out: u32) {
        if let Binding::Pair(dl, dh) = self.bindings[out as usize] {
            let lv = self.pack(low, 14);
            let hv = self.pack(high, 15);
            self.put(dl, lv);
            self.put(dh, hv);
        }
    }

    pub fn half(&mut self, pair: u32, second: bool, kind: &Type, out: u32) {
        if let Binding::Pair(low, high) = self.bindings[pair as usize] {
            let chosen = match second {
                true => high,
                false => low,
            };
            self.unpack(chosen, kind, out);
        }
    }

    fn pack(&mut self, value: u32, scratch: u8) -> u8 {
        match (
            &self.kinds[value as usize].clone(),
            self.bindings[value as usize],
        ) {
            (Type::Float { bits }, Binding::Single(vid)) => {
                let precision = Precision::from(*bits);
                let freg = self.fetch(vid, 16, precision);
                self.encoder.extract(scratch, freg, precision);
                scratch
            }
            (_, Binding::Single(vid)) => {
                let greg = self.get(vid, scratch);
                self.encoder.copy(scratch, greg);
                scratch
            }
            _ => {
                self.encoder.constant(scratch, 0);
                scratch
            }
        }
    }

    fn unpack(&mut self, vid: u32, kind: &Type, out: u32) {
        let raw = self.get(vid, 15);
        match (kind, self.bindings[out as usize]) {
            (Type::Float { bits }, Binding::Single(dst)) => {
                let precision = Precision::from(*bits);
                self.encoder.ingest(16, raw, precision);
                self.place(dst, 16, precision);
            }
            (_, Binding::Single(dst)) => {
                self.normalize(15, raw, kind);
                self.put(dst, 15);
            }
            _ => {}
        }
    }

    pub fn unary(&mut self, op: Unary, value: u32, kind: &Type, out: u32) {
        let Binding::Single(dst) = self.bindings[out as usize] else {
            return;
        };
        let Binding::Single(src) = self.bindings[value as usize] else {
            return;
        };
        if let Type::Float { bits } = *kind {
            let precision = Precision::from(bits);
            let freg = self.fetch(src, 16, precision);
            self.encoder.opposite(16, freg, precision);
            self.place(dst, 16, precision);
            return;
        }
        let reg = self.get(src, 15);
        match (op, kind) {
            (Unary::Neg, _) => self.encoder.negate(15, reg),
            (Unary::Not, Type::Integer { bits: 1, .. }) => self.encoder.flip(15, reg),
            (Unary::Not, _) => self.encoder.invert(15, reg),
        }
        self.normalize(15, 15, kind);
        self.put(dst, 15);
    }

    pub fn binary(&mut self, op: Binary, pair: (u32, u32), kind: &Type, out: u32) {
        let Binding::Single(dst) = self.bindings[out as usize] else {
            return;
        };
        let (Binding::Single(lv), Binding::Single(rv)) = (
            self.bindings[pair.0 as usize],
            self.bindings[pair.1 as usize],
        ) else {
            return;
        };
        let domain = match self.kinds[pair.0 as usize] {
            Type::Void => kind.clone(),
            ref other => other.clone(),
        };
        if let Type::Float { bits } = domain {
            self.decimal(op, (lv, rv, dst), Precision::from(bits));
            return;
        }
        self.integral(op, (lv, rv, dst), (&domain, out));
    }

    fn decimal(&mut self, op: Binary, ids: (u32, u32, u32), precision: Precision) {
        let (lv, rv, dst) = ids;
        let lf = self.fetch(lv, 16, precision);
        let rf = self.fetch(rv, 17, precision);
        match op {
            Binary::Add => self.encoder.arithmetic((16, lf, rf), (0, precision)),
            Binary::Sub => self.encoder.arithmetic((16, lf, rf), (1, precision)),
            Binary::Mul => self.encoder.arithmetic((16, lf, rf), (2, precision)),
            Binary::Div => self.encoder.arithmetic((16, lf, rf), (3, precision)),
            Binary::Rem => self.encoder.modulo((16, lf, rf), precision),
            _ => {
                self.encoder.order(lf, rf, precision);
                let condition = match op {
                    Binary::Eq => Condition::Equal,
                    Binary::Ne => Condition::Distinct,
                    Binary::Lt => Condition::Minus,
                    Binary::Le => Condition::Lower,
                    Binary::Gt => Condition::Greater,
                    _ => Condition::Minimum,
                };
                self.encoder.flag(15, condition);
                self.put(dst, 15);
                return;
            }
        }
        self.place(dst, 16, precision);
    }

    fn integral(&mut self, op: Binary, ids: (u32, u32, u32), types: (&Type, u32)) {
        let (lv, rv, dst) = ids;
        let (domain, out) = types;
        let lr = self.get(lv, 14);
        self.normalize(14, lr, domain);
        let rr = self.get(rv, 15);
        self.normalize(15, rr, domain);
        let (bits, signed) = match *domain {
            Type::Integer { bits, signed } => (bits, signed),
            _ => (64, false),
        };
        match op {
            Binary::Add => self.encoder.add(14, 14, 15),
            Binary::Sub => self.encoder.sub(14, 14, 15),
            Binary::Mul => self.encoder.multiply(14, 14, 15),
            Binary::Div => self.encoder.divide((14, 14, 15), signed),
            Binary::Rem => self.encoder.remainder((14, 14, 15), signed),
            Binary::And => self.encoder.bitand(14, 14, 15),
            Binary::Or => self.encoder.bitor(14, 14, 15),
            Binary::Xor => self.encoder.bitxor(14, 14, 15),
            Binary::Shl => self.encoder.shift((14, 14, 15), (0, bits == 64)),
            Binary::Shr => {
                let code = match signed {
                    true => 2,
                    false => 1,
                };
                self.encoder.shift((14, 14, 15), (code, bits == 64));
            }
            _ => {
                self.encoder.compare(14, 15);
                self.encoder.flag(14, relation(op, signed));
                self.put(dst, 14);
                return;
            }
        }
        let goal = self.kinds[out as usize].clone();
        self.normalize(14, 14, &goal);
        self.put(dst, 14);
    }

    pub fn cast(&mut self, op: Cast, value: u32, types: (&Type, &Type), out: u32) {
        let (from, target) = types;
        let Binding::Single(dst) = self.bindings[out as usize] else {
            return;
        };
        let Binding::Single(src) = self.bindings[value as usize] else {
            return;
        };
        match op {
            Cast::Widen | Cast::Narrow => {
                let reg = self.get(src, 15);
                self.normalize(15, reg, from);
                self.normalize(15, 15, target);
                self.put(dst, 15);
            }
            Cast::Promote | Cast::Elevate => {
                let reg = self.get(src, 15);
                self.normalize(15, reg, from);
                let bits = match *target {
                    Type::Float { bits } => bits,
                    _ => 64,
                };
                let precision = Precision::from(bits);
                match op == Cast::Promote {
                    true => self.encoder.promote(16, 15, precision),
                    false => self.encoder.elevate(16, 15, precision),
                }
                self.place(dst, 16, precision);
            }
            Cast::Truncate | Cast::Demote => {
                let bits = match *from {
                    Type::Float { bits } => bits,
                    _ => 64,
                };
                let precision = Precision::from(bits);
                let freg = self.fetch(src, 16, precision);
                match op == Cast::Truncate {
                    true => self.encoder.truncate(15, freg, precision),
                    false => self.encoder.demote(15, freg, precision),
                }
                self.normalize(15, 15, target);
                self.put(dst, 15);
            }
            Cast::Extend | Cast::Shrink => {
                let widen = op == Cast::Extend;
                let origin = match widen {
                    true => Precision::Single,
                    false => Precision::Double,
                };
                let goal = match widen {
                    true => Precision::Double,
                    false => Precision::Single,
                };
                let freg = self.fetch(src, 16, origin);
                self.encoder.convert(16, freg, widen);
                self.place(dst, 16, goal);
            }
            Cast::Bitcast => self.bitcast(src, (from, target), dst),
        }
    }

    fn bitcast(&mut self, src: u32, types: (&Type, &Type), dst: u32) {
        match types {
            (Type::Float { bits: a }, Type::Float { bits: b }) => {
                let origin = Precision::from(*a);
                let goal = Precision::from(*b);
                let freg = self.fetch(src, 16, origin);
                self.place(dst, freg, goal);
            }
            (Type::Float { bits }, target) => {
                let precision = Precision::from(*bits);
                let freg = self.fetch(src, 16, precision);
                self.encoder.extract(15, freg, precision);
                self.normalize(15, 15, target);
                self.put(dst, 15);
            }
            (_, Type::Float { bits }) => {
                let precision = Precision::from(*bits);
                let greg = self.get(src, 15);
                self.encoder.ingest(16, greg, precision);
                self.place(dst, 16, precision);
            }
            (_, target) => {
                let greg = self.get(src, 15);
                self.normalize(15, greg, target);
                self.put(dst, 15);
            }
        }
    }
}
