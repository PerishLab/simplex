pub mod allocate;
pub mod linear;
mod step;

use self::allocate::{Frame, Location, allocate};
use self::linear::{Access, Parameter, Procedure, Step};
use crate::encode::{Address, Condition, Encoder, Width};
use crate::{Fault, Register};
use simplex_elf::Relocation;

pub struct Emitter<'a> {
    pub(crate) frame: &'a Frame,
    pub(crate) encoder: Encoder,
}

impl<'a> Emitter<'a> {
    pub fn generate(
        proc: &Procedure,
        frame: &'a Frame,
    ) -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
        let mut encoder = Encoder::new();
        for _ in 0..proc.labels {
            let _ = encoder.fresh();
        }
        let mut emitter = Self { frame, encoder };
        emitter.prologue(proc);
        for step in &proc.steps {
            emitter.step(step);
        }
        emitter.encoder.finish()
    }

    pub(crate) fn read(&mut self, value: u32, fallback: Register) -> Register {
        match self.frame.locations[value as usize] {
            Location::Register(reg) => reg,
            Location::Spill(off) => {
                self.encoder
                    .load(fallback, Address::new(Register::Rbp, off), Width::Quad);
                fallback
            }
        }
    }

    pub(crate) fn into(&mut self, dst: Register, value: u32) {
        match self.frame.locations[value as usize] {
            Location::Register(reg) => self.encoder.copy(dst, reg),
            Location::Spill(off) => {
                self.encoder
                    .load(dst, Address::new(Register::Rbp, off), Width::Quad);
            }
        }
    }

    pub(crate) fn write(&mut self, value: u32, src: Register) {
        match self.frame.locations[value as usize] {
            Location::Register(reg) => self.encoder.copy(reg, src),
            Location::Spill(off) => {
                self.encoder
                    .store(Address::new(Register::Rbp, off), src, Width::Quad);
            }
        }
    }

    fn prologue(&mut self, proc: &Procedure) {
        self.encoder.push(Register::Rbp);
        self.encoder.copy(Register::Rbp, Register::Rsp);
        for &reg in &self.frame.preserved {
            self.encoder.push(reg);
        }
        self.encoder.adjust(Register::Rsp, -self.frame.reserve);
        let mut pairs = Vec::new();
        for param in &proc.parameters {
            self.accept(*param, &mut pairs);
        }
        self.shuffle(&mut pairs);
    }

    fn accept(&mut self, param: Parameter, pairs: &mut Vec<(Register, Register)>) {
        match param {
            Parameter::Register { dst, index } => self.incoming(dst, index, pairs),
            Parameter::Stack { dst, offset } => {
                self.encoder.load(
                    Register::R10,
                    Address::new(Register::Rbp, 16 + offset),
                    Width::Quad,
                );
                self.write(dst, Register::R10);
            }
            Parameter::Block {
                slot,
                offset,
                bytes,
            } => {
                let base = self.frame.bases[slot.0 as usize];
                self.encoder
                    .address(Register::R10, Address::new(Register::Rbp, base));
                self.encoder
                    .address(Register::R11, Address::new(Register::Rbp, 16 + offset));
                self.transfer(Register::R10, Register::R11, bytes);
            }
        }
    }

    fn incoming(&mut self, dst: u32, index: usize, pairs: &mut Vec<(Register, Register)>) {
        let src = Register::ARGUMENTS[index];
        match self.frame.locations[dst as usize] {
            Location::Register(target) => pairs.push((target, src)),
            Location::Spill(off) => {
                self.encoder
                    .store(Address::new(Register::Rbp, off), src, Width::Quad);
            }
        }
    }

    pub(crate) fn epilogue(&mut self) {
        self.encoder.adjust(Register::Rsp, self.frame.reserve);
        for &reg in self.frame.preserved.iter().rev() {
            self.encoder.pop(reg);
        }
        self.encoder.pop(Register::Rbp);
        self.encoder.leave();
    }

    pub(crate) fn shuffle(&mut self, moves: &mut Vec<(Register, Register)>) {
        moves.retain(|&(d, s)| d != s);
        while !moves.is_empty() {
            let ready = moves
                .iter()
                .position(|&(dst, _)| !moves.iter().any(|&(_, src)| src == dst));
            match ready {
                Some(idx) => {
                    let (dst, src) = moves.remove(idx);
                    self.encoder.copy(dst, src);
                }
                None => self.cycle(moves),
            }
        }
    }

    fn cycle(&mut self, moves: &mut [(Register, Register)]) {
        let src = moves[0].1;
        self.encoder.copy(Register::R10, src);
        for item in moves.iter_mut() {
            if item.1 == src {
                item.1 = Register::R10;
            }
        }
    }

    fn step(&mut self, step: &Step) {
        match step {
            Step::Mark(label) => self.encoder.bind(*label),
            Step::Constant { dst, value } => {
                self.encoder.constant(Register::Rax, *value);
                self.write(*dst, Register::Rax);
            }
            Step::Move { dst, src } => {
                let s = self.read(*src, Register::Rax);
                self.write(*dst, s);
            }
            Step::Slot { dst, slot, delta } => {
                let off = self.frame.bases[slot.0 as usize] + *delta;
                self.encoder
                    .address(Register::Rax, Address::new(Register::Rbp, off));
                self.write(*dst, Register::Rax);
            }
            Step::Symbol {
                dst,
                name,
                external,
            } => {
                match external {
                    true => self.encoder.global(Register::Rax, name),
                    false => self.encoder.symbol(Register::Rax, name),
                }
                self.write(*dst, Register::Rax);
            }
            Step::Load {
                dst,
                base,
                offset,
                access,
            } => self.load(*dst, *base, *offset, *access),
            Step::Store {
                base,
                offset,
                src,
                width,
            } => {
                let b = self.read(*base, Register::R10);
                let s = self.read(*src, Register::Rax);
                self.encoder.store(Address::new(b, *offset), s, *width);
            }
            Step::Copy { dst, src, bytes } => {
                self.into(Register::R10, *dst);
                self.into(Register::R11, *src);
                self.transfer(Register::R10, Register::R11, *bytes);
            }
            Step::Zero { dst, bytes } => {
                self.into(Register::R10, *dst);
                self.clear(Register::R10, *bytes);
            }
            Step::Binary {
                kind,
                dst,
                lhs,
                rhs,
            } => self.binary(*kind, *dst, *lhs, *rhs),
            Step::Unary { kind, dst, src } => self.unary(*kind, *dst, *src),
            Step::Jump(target) => self.encoder.jump(*target),
            Step::Branch {
                cond,
                truth,
                target,
            } => {
                let reg = self.read(*cond, Register::Rax);
                self.encoder.test(reg);
                let code = match truth {
                    true => Condition::Distinct,
                    false => Condition::Equal,
                };
                self.encoder.branch(code, *target);
            }
            Step::Invoke(call) => self.invoke(call),
            Step::Finish(ending) => self.finish(*ending),
        }
    }

    fn load(&mut self, dst: u32, base: u32, offset: i32, access: Access) {
        let b = self.read(base, Register::R10);
        let addr = Address::new(b, offset);
        match access.signed {
            true => self.encoder.signed(Register::Rax, addr, access.width),
            false => self.encoder.load(Register::Rax, addr, access.width),
        }
        self.write(dst, Register::Rax);
    }

    pub(crate) fn transfer(&mut self, dst: Register, src: Register, bytes: u32) {
        let mut cursor = 0i32;
        let mut rest = bytes;
        while rest >= 8 {
            self.encoder
                .load(Register::Rax, Address::new(src, cursor), Width::Quad);
            self.encoder
                .store(Address::new(dst, cursor), Register::Rax, Width::Quad);
            cursor += 8;
            rest -= 8;
        }
        for step in [4u32, 2, 1] {
            if rest >= step {
                let width = Width::from(u64::from(step));
                self.encoder
                    .load(Register::Rax, Address::new(src, cursor), width);
                self.encoder
                    .store(Address::new(dst, cursor), Register::Rax, width);
                cursor += step as i32;
                rest -= step;
            }
        }
    }

    fn clear(&mut self, dst: Register, bytes: u32) {
        if bytes == 0 {
            return;
        }
        self.encoder.constant(Register::Rax, 0);
        let mut cursor = 0i32;
        let mut rest = bytes;
        while rest >= 8 {
            self.encoder
                .store(Address::new(dst, cursor), Register::Rax, Width::Quad);
            cursor += 8;
            rest -= 8;
        }
        for step in [4u32, 2, 1] {
            if rest >= step {
                let width = Width::from(u64::from(step));
                self.encoder
                    .store(Address::new(dst, cursor), Register::Rax, width);
                cursor += step as i32;
                rest -= step;
            }
        }
    }
}

pub fn emit(proc: &Procedure) -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
    let frame = allocate(proc);
    Emitter::generate(proc, &frame)
}
