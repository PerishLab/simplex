use super::Emitter;
use super::shape::Value;
use crate::intel::Register;
use simplex_ir::{Cast, Type};

impl Emitter<'_> {
    pub(crate) fn cast(&mut self, out: u32, op: Cast, src: u32, types: (&Type, &Type)) {
        match op {
            Cast::Widen | Cast::Narrow => self.resize(out, src, types),
            Cast::Promote => self.into(out, src, types.0, true),
            Cast::Elevate => self.into(out, src, types.0, false),
            Cast::Truncate => self.from(out, src, types.1, true),
            Cast::Demote => self.from(out, src, types.1, false),
            Cast::Extend | Cast::Shrink => self.precision(out, src),
            Cast::Bitcast => self.bitcast(out, src, types.1),
        }
    }

    fn resize(&mut self, out: u32, src: u32, types: (&Type, &Type)) {
        if let (Value::General(d), Value::General(s)) =
            (self.frame.value(out), self.frame.value(src))
        {
            self.read(Register::Rax, s);
            self.normalize(Register::Rax, types.0);
            self.normalize(Register::Rax, types.1);
            self.write(d, Register::Rax);
        }
    }

    fn into(&mut self, out: u32, src: u32, origin: &Type, signed: bool) {
        let (Value::Float { reg: d, prec }, Value::General(s)) =
            (self.frame.value(out), self.frame.value(src))
        else {
            return;
        };
        self.read(Register::Rax, s);
        self.normalize(Register::Rax, origin);
        match signed {
            true => self.encoder.promote(0, Register::Rax, prec),
            false => self.encoder.elevate(0, Register::Rax, prec),
        }
        self.deposit(d, 0);
    }

    fn from(&mut self, out: u32, src: u32, target: &Type, signed: bool) {
        let (Value::General(d), Value::Float { reg: s, prec }) =
            (self.frame.value(out), self.frame.value(src))
        else {
            return;
        };
        self.fetch(0, s);
        match signed {
            true => self.encoder.truncate(Register::Rax, 0, prec),
            false => self.encoder.demote(Register::Rax, 0, prec),
        }
        self.normalize(Register::Rax, target);
        self.write(d, Register::Rax);
    }

    fn precision(&mut self, out: u32, src: u32) {
        if let (Value::Float { reg: d, prec }, Value::Float { reg: s, .. }) =
            (self.frame.value(out), self.frame.value(src))
        {
            self.fetch(0, s);
            self.encoder.convert(0, 0, prec);
            self.deposit(d, 0);
        }
    }
}
