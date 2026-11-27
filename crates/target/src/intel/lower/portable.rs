use super::Emitter;
use super::abi::{self, Callee};
use super::call::Source;
use super::shape::Value;
use crate::intel::{Address, Flavor, Register, Width};
use crate::machine::Location;
use simplex_ir::{Instruction, Type};

pub(crate) fn space(flavor: Flavor, inputs: &[Type], output: &Type) -> u32 {
    match flavor {
        Flavor::Linux => sysv(inputs, output),
        Flavor::Windows => footprint(inputs, output),
    }
}

fn sysv(inputs: &[Type], output: &Type) -> u32 {
    let mut gp = usize::from(abi::indirect(Flavor::Linux, output));
    let mut fp = 0usize;
    let mut off = 0u32;
    for kind in inputs {
        match kind {
            Type::Void | Type::Aggregate { size: 0, .. } => {}
            Type::Float { .. } if fp < 8 => fp += 1,
            Type::Float { .. } => off += 8,
            Type::Pair | Type::Aggregate { size: 9..=16, .. } if gp + 2 <= 6 => gp += 2,
            Type::Pair | Type::Aggregate { size: 9..=16, .. } => off += 16,
            Type::Aggregate {
                size: s @ 17..,
                align,
                ..
            } => {
                let mask = (*align).max(8) - 1;
                off = (off + mask) & !mask;
                off += (*s + 7) & !7;
            }
            _ if gp < 6 => gp += 1,
            _ => off += 8,
        }
    }
    off
}

fn footprint(inputs: &[Type], output: &Type) -> u32 {
    let mut slot = usize::from(abi::indirect(Flavor::Windows, output));
    for kind in inputs {
        if !matches!(kind, Type::Void | Type::Aggregate { size: 0, .. }) {
            slot += 1;
        }
    }
    let args = match slot > 4 {
        true => 32 + ((slot - 4) * 8) as u32,
        false => 32,
    };
    let mut copy = (args + 15) & !15;
    if matches!(output, Type::Pair) {
        copy += 16;
    }
    for kind in inputs {
        match kind {
            Type::Pair => copy += 16,
            Type::Aggregate { size, .. } if !matches!(*size, 0 | 1 | 2 | 4 | 8) => {
                copy += (*size + 15) & !15;
            }
            _ => {}
        }
    }
    copy
}

impl Emitter<'_> {
    pub(crate) fn portable(&mut self, inst: &Instruction, callee: Callee, call: (&[u32], &[Type])) {
        let regs = Register::windows();
        let mut gp: Vec<(Register, Source)> = Vec::new();
        let mut fp = Vec::new();
        let mut mirrors = Vec::new();
        let mut count = usize::from(abi::indirect(Flavor::Windows, &inst.kind));
        for kind in call.1 {
            if !matches!(kind, Type::Void | Type::Aggregate { size: 0, .. }) {
                count += 1;
            }
        }
        let end = match count > 4 {
            true => 32 + ((count - 4) * 8) as i32,
            false => 32,
        };
        let mut scratch = (end + 15) & !15;
        let mut capture = None;
        let mut idx = 0usize;
        if abi::indirect(Flavor::Windows, &inst.kind) {
            match self.frame.value(inst.value) {
                Value::Aggregate { slot, .. } => {
                    gp.push((regs[0], Source::Frame(self.frame.base(slot))));
                }
                Value::Pair { .. } => {
                    capture = Some(scratch);
                    gp.push((regs[0], Source::Stack(scratch)));
                    scratch += 16;
                }
                _ => {}
            }
            idx = 1;
        }
        for (&arg, kind) in call.0.iter().zip(call.1) {
            if matches!(kind, Type::Void | Type::Aggregate { size: 0, .. }) {
                continue;
            }
            let src = self.stage(arg, &mut scratch);
            let disp = 32 + ((idx.saturating_sub(4)) * 8) as i32;
            match (idx < 4, self.frame.value(arg), src) {
                (true, Value::Float { reg, prec }, None) => {
                    fp.push((Location::Float(idx as u8), self.frame.location(reg)));
                    mirrors.push((regs[idx], idx as u8, prec));
                }
                (false, Value::Float { reg, prec }, None) => {
                    self.fetch(0, reg);
                    self.encoder
                        .deposit(Address::new(Register::Rsp, disp), 0, prec);
                }
                (true, _, Some(item)) => gp.push((regs[idx], item)),
                (false, _, Some(item)) => self.outgoing(disp, item, kind),
                _ => {}
            }
            idx += 1;
        }
        self.dispatch(callee, gp, fp, &mirrors);
        self.reap(inst, capture);
    }

    fn stage(&mut self, arg: u32, scratch: &mut i32) -> Option<Source> {
        match self.frame.value(arg) {
            Value::Float { .. } | Value::Empty => None,
            Value::General(r) => Some(Source::Reg(r)),
            Value::Aggregate { slot, size } if matches!(size, 1 | 2 | 4 | 8) => {
                Some(Source::Packed {
                    slot,
                    delta: 0,
                    bytes: size,
                })
            }
            Value::Pair { low, high } => {
                let off = *scratch;
                *scratch += 16;
                self.read(Register::Rax, low);
                self.encoder
                    .store(Address::new(Register::Rsp, off), Register::Rax, Width::Quad);
                self.read(Register::Rax, high);
                self.encoder.store(
                    Address::new(Register::Rsp, off + 8),
                    Register::Rax,
                    Width::Quad,
                );
                Some(Source::Stack(off))
            }
            Value::Aggregate { slot, size } => {
                let off = *scratch;
                *scratch += ((size + 15) & !15) as i32;
                self.encoder
                    .address(Register::R10, Address::new(Register::Rsp, off));
                self.encoder.address(
                    Register::R11,
                    Address::new(Register::Rbp, self.frame.base(slot)),
                );
                self.encoder.replicate(Register::R10, Register::R11, size);
                Some(Source::Stack(off))
            }
        }
    }

    fn outgoing(&mut self, disp: i32, src: Source, kind: &Type) {
        match src {
            Source::Reg(r) => {
                self.read(Register::Rax, r);
                self.normalize(Register::Rax, kind);
            }
            Source::Spilled(off) => {
                self.encoder
                    .load(Register::Rax, Address::new(Register::Rbp, off), Width::Quad);
            }
            Source::Frame(off) => {
                self.encoder
                    .address(Register::Rax, Address::new(Register::Rbp, off));
            }
            Source::Stack(off) => {
                self.encoder
                    .address(Register::Rax, Address::new(Register::Rsp, off));
            }
            Source::Packed { slot, delta, bytes } => {
                let addr = Address::new(Register::Rbp, self.frame.base(slot) + delta);
                self.encoder.pack(Register::Rax, addr, bytes);
            }
        }
        self.encoder.store(
            Address::new(Register::Rsp, disp),
            Register::Rax,
            Width::Quad,
        );
    }

    fn reap(&mut self, inst: &Instruction, capture: Option<i32>) {
        match self.frame.value(inst.value) {
            Value::Empty => {}
            Value::General(dst) => {
                self.normalize(Register::Rax, &inst.kind);
                self.write(dst, Register::Rax);
            }
            Value::Float { reg, .. } => self.deposit(reg, 0),
            Value::Pair { low, high } => {
                if let Some(off) = capture {
                    self.encoder
                        .load(Register::Rax, Address::new(Register::Rsp, off), Width::Quad);
                    self.encoder.load(
                        Register::R10,
                        Address::new(Register::Rsp, off + 8),
                        Width::Quad,
                    );
                    self.write(low, Register::Rax);
                    self.write(high, Register::R10);
                }
            }
            Value::Aggregate { slot, size } if matches!(size, 1 | 2 | 4 | 8) => {
                let dst = Address::new(Register::Rbp, self.frame.base(slot));
                self.encoder.store(dst, Register::Rax, Width::bytes(size));
            }
            Value::Aggregate { .. } => {}
        }
    }
}
