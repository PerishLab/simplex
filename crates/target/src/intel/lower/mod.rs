mod abi;
mod call;
mod cast;
mod control;
mod flow;
mod memory;
mod ops;
mod portable;
pub(crate) mod shape;

use self::shape::Frame;
use crate::Fault;
use crate::intel::encode::Encoder;
use crate::intel::link::Catalog;
use crate::intel::{Address, Label, Precision, Register, Width};
use crate::machine::Location;
use simplex_ir::{Function, Type};
use simplex_object::Relocation;
use std::collections::BTreeMap;

pub(crate) struct Emitter<'a> {
    pub(crate) frame: &'a Frame,
    pub(crate) symbols: &'a Catalog,
    pub(crate) encoder: Encoder,
    pub(crate) labels: BTreeMap<u32, Label>,
    pub(crate) blocks: BTreeMap<u32, usize>,
}

impl<'a> Emitter<'a> {
    pub(crate) fn generate(
        func: &Function,
        frame: &'a Frame,
        symbols: &'a Catalog,
    ) -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
        let mut encoder = Encoder::new();
        let mut labels = BTreeMap::new();
        let mut blocks = BTreeMap::new();
        for (idx, block) in func.blocks.iter().enumerate() {
            labels.insert(block.id, encoder.fresh());
            blocks.insert(block.id, idx);
        }
        let mut emitter = Self {
            frame,
            symbols,
            encoder,
            labels,
            blocks,
        };
        emitter.prologue(func);
        if let Some(&first) = frame.order.first()
            && func.blocks[first].id != func.blocks[0].id
        {
            emitter.encoder.jump(emitter.labels[&func.blocks[0].id]);
        }
        for &idx in &frame.order {
            emitter.block(func, &func.blocks[idx])?;
        }
        emitter.encoder.finish()
    }

    pub(crate) fn read(&mut self, dst: Register, reg: u32) {
        match self.frame.location(reg) {
            Location::General(code) => self.encoder.copy(dst, Register::decode(code)),
            Location::Spill(off) => {
                let addr = Address::new(Register::Rbp, self.frame.spill(off));
                self.encoder.load(dst, addr, Width::Quad);
            }
            Location::Float(xmm) => self.encoder.extract(dst, xmm, Precision::Double),
        }
    }

    pub(crate) fn write(&mut self, reg: u32, src: Register) {
        match self.frame.location(reg) {
            Location::General(code) => self.encoder.copy(Register::decode(code), src),
            Location::Spill(off) => {
                let addr = Address::new(Register::Rbp, self.frame.spill(off));
                self.encoder.store(addr, src, Width::Quad);
            }
            Location::Float(xmm) => self.encoder.ingest(xmm, src, Precision::Double),
        }
    }

    pub(crate) fn fetch(&mut self, dst: u8, reg: u32) {
        match self.frame.location(reg) {
            Location::Float(xmm) => self.encoder.slide(dst, xmm),
            Location::Spill(off) => {
                let addr = Address::new(Register::Rbp, self.frame.spill(off));
                self.encoder.fetch(dst, addr, Precision::Double);
            }
            Location::General(code) => {
                self.encoder
                    .ingest(dst, Register::decode(code), Precision::Double);
            }
        }
    }

    pub(crate) fn deposit(&mut self, reg: u32, src: u8) {
        match self.frame.location(reg) {
            Location::Float(xmm) => self.encoder.slide(xmm, src),
            Location::Spill(off) => {
                let addr = Address::new(Register::Rbp, self.frame.spill(off));
                self.encoder.deposit(addr, src, Precision::Double);
            }
            Location::General(code) => {
                self.encoder
                    .extract(Register::decode(code), src, Precision::Double);
            }
        }
    }

    pub(crate) fn normalize(&mut self, reg: Register, kind: &Type) {
        match kind {
            Type::Integer { bits: 1, .. } => {
                self.encoder.constant(Register::R11, 1);
                self.encoder.mask(reg, Register::R11);
            }
            Type::Integer {
                bits: b @ (8 | 16 | 32),
                signed: false,
            } => {
                self.encoder.extend(reg, reg, Width::bits(*b));
            }
            Type::Integer {
                bits: b @ (8 | 16 | 32),
                signed: true,
            } => {
                self.encoder.widen(reg, reg, Width::bits(*b));
            }
            _ => {}
        }
    }

    pub(crate) fn shuffle(&mut self, moves: &mut Vec<(Location, Location)>) {
        let scratch = Location::General(Register::R10.code());
        self.sequence(moves, scratch);
    }

    pub(crate) fn rearrange(&mut self, moves: &mut Vec<(Location, Location)>) {
        let scratch = Location::General(Register::Rax.code());
        self.sequence(moves, scratch);
    }

    fn sequence(&mut self, moves: &mut Vec<(Location, Location)>, scratch: Location) {
        moves.retain(|&(d, s)| d != s);
        while !moves.is_empty() {
            let ready = moves
                .iter()
                .position(|&(dst, _)| !moves.iter().any(|&(_, src)| src == dst));
            match ready {
                Some(idx) => {
                    let (dst, src) = moves.remove(idx);
                    self.glide(dst, src);
                }
                None => self.sever(moves, scratch),
            }
        }
    }

    fn sever(&mut self, moves: &mut [(Location, Location)], scratch: Location) {
        let src = moves[0].1;
        self.glide(scratch, src);
        for item in moves.iter_mut() {
            if item.1 == src {
                item.1 = scratch;
            }
        }
    }

    fn glide(&mut self, dst: Location, src: Location) {
        match (dst, src) {
            (Location::General(d), Location::General(s)) => {
                self.encoder.copy(Register::decode(d), Register::decode(s));
            }
            (Location::Float(d), Location::Float(s)) => self.encoder.slide(d, s),
            (Location::General(d), Location::Spill(s)) => {
                let addr = Address::new(Register::Rbp, self.frame.spill(s));
                self.encoder.load(Register::decode(d), addr, Width::Quad);
            }
            (Location::Spill(d), Location::General(s)) => {
                let addr = Address::new(Register::Rbp, self.frame.spill(d));
                self.encoder.store(addr, Register::decode(s), Width::Quad);
            }
            (Location::Float(d), Location::Spill(s)) => {
                let addr = Address::new(Register::Rbp, self.frame.spill(s));
                self.encoder.fetch(d, addr, Precision::Double);
            }
            (Location::Spill(d), Location::Float(s)) => {
                let addr = Address::new(Register::Rbp, self.frame.spill(d));
                self.encoder.deposit(addr, s, Precision::Double);
            }
            (Location::General(d), Location::Float(s)) => {
                self.encoder
                    .extract(Register::decode(d), s, Precision::Double);
            }
            (Location::Float(d), Location::General(s)) => {
                self.encoder
                    .ingest(d, Register::decode(s), Precision::Double);
            }
            (Location::Spill(d), Location::Spill(s)) => {
                let first = Address::new(Register::Rbp, self.frame.spill(s));
                let second = Address::new(Register::Rbp, self.frame.spill(d));
                self.encoder.load(Register::Rax, first, Width::Quad);
                self.encoder.store(second, Register::Rax, Width::Quad);
            }
        }
    }
}
