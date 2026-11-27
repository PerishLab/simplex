use super::Emitter;
use super::abi::{self, Callee};
use super::shape::Value;
use crate::intel::{Address, Flavor, Precision, Register, Width};
use crate::machine::Location;
use simplex_ir::{Instruction, Type};

pub(crate) enum Source {
    Reg(u32),
    Spilled(i32),
    Frame(i32),
    Stack(i32),
    Packed { slot: usize, delta: i32, bytes: u32 },
}

impl Emitter<'_> {
    pub(crate) fn invoke(&mut self, inst: &Instruction, callee: Callee, call: (&[u32], &[Type])) {
        match self.frame.flavor {
            Flavor::Linux => self.sysv(inst, callee, call),
            Flavor::Windows => self.portable(inst, callee, call),
        }
    }

    fn sysv(&mut self, inst: &Instruction, callee: Callee, call: (&[u32], &[Type])) {
        let regs = Register::linux();
        let mut gp: Vec<(Register, Source)> = Vec::new();
        let mut fp = Vec::new();
        let mut gidx = 0usize;
        let mut fidx = 0usize;
        let mut off = 0i32;
        if abi::indirect(Flavor::Linux, &inst.kind)
            && let Value::Aggregate { slot, .. } = self.frame.value(inst.value)
        {
            gp.push((regs[0], Source::Frame(self.frame.base(slot))));
            gidx = 1;
        }
        for (&arg, kind) in call.0.iter().zip(call.1) {
            match (self.frame.value(arg), kind) {
                (Value::Empty, _) => {}
                (Value::Float { reg, .. }, _) if fidx < 8 => {
                    fp.push((Location::Float(fidx as u8), self.frame.location(reg)));
                    fidx += 1;
                }
                (Value::Float { reg, prec }, _) => {
                    self.fetch(0, reg);
                    self.encoder
                        .deposit(Address::new(Register::Rsp, off), 0, prec);
                    off += 8;
                }
                (Value::Pair { low, high }, _) if gidx + 2 <= 6 => {
                    gp.push((regs[gidx], Source::Reg(low)));
                    gp.push((regs[gidx + 1], Source::Reg(high)));
                    gidx += 2;
                }
                (Value::Pair { low, high }, _) => {
                    self.read(Register::Rax, low);
                    self.encoder.store(
                        Address::new(Register::Rsp, off),
                        Register::Rax,
                        Width::Quad,
                    );
                    self.read(Register::Rax, high);
                    self.encoder.store(
                        Address::new(Register::Rsp, off + 8),
                        Register::Rax,
                        Width::Quad,
                    );
                    off += 16;
                }
                (Value::Aggregate { slot, size }, _) if size <= 8 && gidx < 6 => {
                    gp.push((
                        regs[gidx],
                        Source::Packed {
                            slot,
                            delta: 0,
                            bytes: size,
                        },
                    ));
                    gidx += 1;
                }
                (Value::Aggregate { slot, size }, _) if size <= 16 && gidx + 2 <= 6 => {
                    gp.push((
                        regs[gidx],
                        Source::Packed {
                            slot,
                            delta: 0,
                            bytes: 8,
                        },
                    ));
                    gp.push((
                        regs[gidx + 1],
                        Source::Packed {
                            slot,
                            delta: 8,
                            bytes: size - 8,
                        },
                    ));
                    gidx += 2;
                }
                (Value::Aggregate { slot, size }, _) if size <= 16 => {
                    self.spill(slot, size, &mut off);
                }
                (Value::Aggregate { slot, size }, Type::Aggregate { align, .. }) => {
                    let mask = (*align).max(8) as i32 - 1;
                    off = (off + mask) & !mask;
                    self.encoder
                        .address(Register::R10, Address::new(Register::Rsp, off));
                    self.encoder.address(
                        Register::R11,
                        Address::new(Register::Rbp, self.frame.base(slot)),
                    );
                    self.encoder.replicate(Register::R10, Register::R11, size);
                    off += ((size + 7) & !7) as i32;
                }
                (Value::General(src), _) if gidx < 6 => {
                    gp.push((regs[gidx], Source::Reg(src)));
                    gidx += 1;
                }
                (Value::General(src), _) => {
                    self.read(Register::Rax, src);
                    self.normalize(Register::Rax, kind);
                    self.encoder.store(
                        Address::new(Register::Rsp, off),
                        Register::Rax,
                        Width::Quad,
                    );
                    off += 8;
                }
                _ => {}
            }
        }
        self.dispatch(callee, gp, fp, &[]);
        self.harvest(inst);
    }

    fn spill(&mut self, slot: usize, size: u32, off: &mut i32) {
        let base = self.frame.base(slot);
        self.encoder.pack(
            Register::Rax,
            Address::new(Register::Rbp, base),
            size.min(8),
        );
        self.encoder.store(
            Address::new(Register::Rsp, *off),
            Register::Rax,
            Width::Quad,
        );
        if size > 8 {
            self.encoder.pack(
                Register::Rax,
                Address::new(Register::Rbp, base + 8),
                size - 8,
            );
            self.encoder.store(
                Address::new(Register::Rsp, *off + 8),
                Register::Rax,
                Width::Quad,
            );
        }
        *off += match size > 8 {
            true => 16,
            false => 8,
        };
    }

    pub(crate) fn dispatch(
        &mut self,
        callee: Callee,
        gp: Vec<(Register, Source)>,
        mut fp: Vec<(Location, Location)>,
        mirrors: &[(Register, u8, Precision)],
    ) {
        self.rearrange(&mut fp);
        let mut moves = Vec::new();
        let mut late = Vec::new();
        for (dst, src) in gp {
            self.classify(dst, src, &mut moves, &mut late);
        }
        if let Callee::Indirect(ptr) = &callee
            && let Value::General(r) = self.frame.value(*ptr)
        {
            self.read(Register::R11, r);
        }
        self.shuffle(&mut moves);
        for (dst, src) in late {
            self.materialize(dst, src);
        }
        for &(dst, xmm, prec) in mirrors {
            self.encoder.extract(dst, xmm, prec);
        }
        match callee {
            Callee::Direct(sym) => self.encoder.call(&sym),
            Callee::Indirect(_) => self.encoder.indirect(Register::R11),
        }
    }

    fn classify(
        &self,
        dst: Register,
        src: Source,
        moves: &mut Vec<(Location, Location)>,
        late: &mut Vec<(Register, Source)>,
    ) {
        match src {
            Source::Reg(r) => match self.frame.location(r) {
                Location::General(code) => {
                    moves.push((Location::General(dst.code()), Location::General(code)));
                }
                Location::Spill(off) => late.push((dst, Source::Spilled(self.frame.spill(off)))),
                Location::Float(_) => {}
            },
            other => late.push((dst, other)),
        }
    }

    fn materialize(&mut self, dst: Register, src: Source) {
        match src {
            Source::Spilled(off) => {
                self.encoder
                    .load(dst, Address::new(Register::Rbp, off), Width::Quad);
            }
            Source::Frame(off) => self.encoder.address(dst, Address::new(Register::Rbp, off)),
            Source::Stack(off) => self.encoder.address(dst, Address::new(Register::Rsp, off)),
            Source::Packed { slot, delta, bytes } => {
                let addr = Address::new(Register::Rbp, self.frame.base(slot) + delta);
                self.encoder.pack(dst, addr, bytes);
            }
            Source::Reg(r) => self.read(dst, r),
        }
    }

    fn harvest(&mut self, inst: &Instruction) {
        match self.frame.value(inst.value) {
            Value::Empty | Value::Aggregate { size: 17.., .. } => {}
            Value::General(dst) => {
                self.normalize(Register::Rax, &inst.kind);
                self.write(dst, Register::Rax);
            }
            Value::Float { reg, .. } => self.deposit(reg, 0),
            Value::Pair { low, high } => {
                self.encoder.copy(Register::R10, Register::Rdx);
                self.write(low, Register::Rax);
                self.write(high, Register::R10);
            }
            Value::Aggregate { slot, size } if size <= 8 => {
                let dst = Address::new(Register::Rbp, self.frame.base(slot));
                self.encoder.unpack(dst, Register::Rax, size);
            }
            Value::Aggregate { slot, size } => {
                let base = self.frame.base(slot);
                self.encoder.store(
                    Address::new(Register::Rbp, base),
                    Register::Rax,
                    Width::Quad,
                );
                self.encoder.unpack(
                    Address::new(Register::Rbp, base + 8),
                    Register::Rdx,
                    size - 8,
                );
            }
        }
    }
}
