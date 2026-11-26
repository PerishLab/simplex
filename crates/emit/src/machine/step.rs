use super::Emitter;
use super::allocate::Location;
use super::linear::{Binary, Callee, Ending, Invocation, Outgoing, Unary};
use crate::Register;
use crate::encode::media::Precision;
use crate::encode::{Address, Width};

impl Emitter<'_> {
    pub(crate) fn binary(&mut self, kind: Binary, dst: u32, lhs: u32, rhs: u32) {
        self.into(Register::Rax, lhs);
        self.into(Register::Rcx, rhs);
        match kind {
            Binary::Add => self.encoder.add(Register::Rax, Register::Rcx),
            Binary::Sub => self.encoder.sub(Register::Rax, Register::Rcx),
            Binary::Mul => self.encoder.multiply(Register::Rax, Register::Rcx),
            Binary::Quotient(signed) => self.encoder.divide(signed),
            Binary::Remainder(signed) => {
                self.encoder.divide(signed);
                self.encoder.copy(Register::Rax, Register::Rdx);
            }
            Binary::Conjunction => self.encoder.bitand(Register::Rax, Register::Rcx),
            Binary::Disjunction => self.encoder.bitor(Register::Rax, Register::Rcx),
            Binary::Exclusive => self.encoder.bitxor(Register::Rax, Register::Rcx),
            Binary::Shift(dir) => self.encoder.shift(dir, Register::Rax),
            Binary::Compare(cond) => {
                self.encoder.compare(Register::Rax, Register::Rcx);
                self.encoder.flag(Register::Rax, cond);
            }
            Binary::Float(op, prec) => {
                self.encoder.ingest(0, Register::Rax, prec);
                self.encoder.ingest(1, Register::Rcx, prec);
                self.encoder.scalar(op, prec);
                self.encoder.extract(Register::Rax, 0, prec);
            }
            Binary::Order(op, prec) => {
                self.encoder.ingest(0, Register::Rax, prec);
                self.encoder.ingest(1, Register::Rcx, prec);
                self.encoder.relation(Register::Rax, op, prec);
            }
        }
        self.write(dst, Register::Rax);
    }

    pub(crate) fn unary(&mut self, kind: Unary, dst: u32, src: u32) {
        self.into(Register::Rcx, src);
        match kind {
            Unary::Negate => {
                self.encoder.copy(Register::Rax, Register::Rcx);
                self.encoder.negate(Register::Rax);
            }
            Unary::Invert => {
                self.encoder.copy(Register::Rax, Register::Rcx);
                self.encoder.invert(Register::Rax);
            }
            Unary::Not => {
                self.encoder
                    .extend(Register::Rax, Register::Rcx, Width::Byte);
                self.encoder.constant(Register::Rdx, 1);
                self.encoder.bitxor(Register::Rax, Register::Rdx);
            }
            Unary::Extend(width) => self.encoder.extend(Register::Rax, Register::Rcx, width),
            Unary::Widen(width) => self.encoder.widen(Register::Rax, Register::Rcx, width),
            Unary::Promote(prec) => self.encoder.promote(Register::Rax, Register::Rcx, prec),
            Unary::Unsigned(prec) => self.encoder.unsigned(Register::Rax, Register::Rcx, prec),
            Unary::Truncate(prec) => self.encoder.truncate(Register::Rax, Register::Rcx, prec),
            Unary::Resize(prec) => self.encoder.resize(Register::Rax, Register::Rcx, prec),
            Unary::Flip(prec) => {
                let sign = match prec {
                    Precision::Single => 1i64 << 31,
                    Precision::Double => i64::MIN,
                };
                self.encoder.copy(Register::Rax, Register::Rcx);
                self.encoder.constant(Register::Rdx, sign);
                self.encoder.bitxor(Register::Rax, Register::Rdx);
            }
        }
        self.write(dst, Register::Rax);
    }

    pub(crate) fn invoke(&mut self, call: &Invocation) {
        for item in &call.stack {
            match *item {
                Outgoing::Scalar { src, offset } => {
                    let s = self.read(src, Register::Rax);
                    self.encoder
                        .store(Address::new(Register::Rsp, offset as i32), s, Width::Quad);
                }
                Outgoing::Block {
                    base,
                    offset,
                    bytes,
                } => {
                    self.into(Register::R11, base);
                    self.encoder
                        .address(Register::R10, Address::new(Register::Rsp, offset as i32));
                    self.transfer(Register::R10, Register::R11, bytes);
                }
            }
        }
        for (idx, &(arg, prec)) in call.floats.iter().enumerate() {
            let src = self.read(arg, Register::Rax);
            self.encoder.ingest(idx as u8, src, prec);
        }
        if let Callee::Indirect(reg) = call.callee {
            self.into(Register::R11, reg);
        }
        let mut moves = Vec::new();
        for (idx, &arg) in call.registers.iter().enumerate() {
            let target = Register::ARGUMENTS[idx];
            match self.frame.locations[arg as usize] {
                Location::Register(src) => moves.push((target, src)),
                Location::Spill(_) => {}
            }
        }
        self.shuffle(&mut moves);
        for (idx, &arg) in call.registers.iter().enumerate() {
            if let Location::Spill(off) = self.frame.locations[arg as usize] {
                self.encoder.load(
                    Register::ARGUMENTS[idx],
                    Address::new(Register::Rbp, off),
                    Width::Quad,
                );
            }
        }
        match &call.callee {
            Callee::Direct { symbol, external } => self.encoder.call(symbol, *external),
            Callee::Indirect(_) => self.encoder.indirect(Register::R11),
        }
        self.results(call);
    }

    fn results(&mut self, call: &Invocation) {
        if let Some(prec) = call.media {
            if let Some(first) = call.first {
                self.encoder.extract(Register::Rax, 0, prec);
                self.write(first, Register::Rax);
            }
            return;
        }
        match (call.first, call.second) {
            (Some(first), Some(second)) => {
                self.encoder.copy(Register::R10, Register::Rdx);
                self.write(first, Register::Rax);
                self.write(second, Register::R10);
            }
            (Some(first), None) => self.write(first, Register::Rax),
            (None, Some(second)) => self.write(second, Register::Rdx),
            (None, None) => {}
        }
    }

    pub(crate) fn finish(&mut self, ending: Ending) {
        match ending {
            Ending::Void => {}
            Ending::Single(v) => self.into(Register::Rax, v),
            Ending::Pair(first, second) => {
                self.into(Register::R10, second);
                self.into(Register::Rax, first);
                self.encoder.copy(Register::Rdx, Register::R10);
            }
            Ending::Indirect {
                target,
                source,
                bytes,
            } => {
                self.into(Register::R10, target);
                self.into(Register::R11, source);
                self.transfer(Register::R10, Register::R11, bytes);
                self.into(Register::Rax, target);
            }
        }
        self.epilogue();
    }
}
