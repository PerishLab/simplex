use super::Context;
use crate::arm::{Precision, Width};
use crate::machine::Location;

#[derive(Default)]
pub struct Moves {
    pub general: Vec<(Location, Location)>,
    pub media: Vec<(Location, Location, Precision)>,
    pub blocks: Vec<(usize, usize, u32)>,
}

impl Context<'_> {
    pub fn blocks(&mut self, moves: &mut Vec<(usize, usize, u32)>) {
        while !moves.is_empty() {
            let leaf = moves
                .iter()
                .position(|&(dst, _, _)| !moves.iter().any(|&(_, src, _)| src == dst));
            match leaf {
                Some(index) => self.transfer(moves.remove(index)),
                None => self.exchange(moves),
            }
        }
    }

    fn transfer(&mut self, entry: (usize, usize, u32)) {
        let (dst, src, size) = entry;
        let d = i64::from(self.locals[dst].offset);
        let s = i64::from(self.locals[src].offset);
        self.encoder.displace(14, 31, d);
        self.encoder.displace(17, 31, s);
        self.encoder.transfer(14, 17, size);
    }

    fn exchange(&mut self, moves: &mut Vec<(usize, usize, u32)>) {
        let (dst, src, size) = moves.remove(0);
        let d = i64::from(self.locals[dst].offset);
        let s = i64::from(self.locals[src].offset);
        for cursor in 0..size {
            self.encoder
                .load(14, (31, d + i64::from(cursor)), (Width::Byte, false));
            self.encoder
                .load(15, (31, s + i64::from(cursor)), (Width::Byte, false));
            self.encoder
                .store(14, (31, s + i64::from(cursor)), Width::Byte);
            self.encoder
                .store(15, (31, d + i64::from(cursor)), Width::Byte);
        }
        for item in moves.iter_mut() {
            if item.1 == dst {
                item.1 = src;
            }
        }
        moves.retain(|&(a, b, _)| a != b);
    }

    pub fn shuffle(&mut self, moves: &mut Vec<(Location, Location)>) {
        moves.retain(|&(d, s)| d != s);
        while !moves.is_empty() {
            let leaf = moves
                .iter()
                .position(|&(dst, _)| !moves.iter().any(|&(_, src)| src == dst));
            match leaf {
                Some(index) => {
                    let (dst, src) = moves.remove(index);
                    self.step(dst, src);
                }
                None => self.unstick(moves),
            }
        }
    }

    fn unstick(&mut self, moves: &mut [(Location, Location)]) {
        let src = moves[0].1;
        self.step(Location::General(14), src);
        for item in moves.iter_mut() {
            if item.1 == src {
                item.1 = Location::General(14);
            }
        }
    }

    fn step(&mut self, destination: Location, source: Location) {
        match (destination, source) {
            (Location::General(d), Location::General(s)) => self.encoder.copy(d, s),
            (Location::General(d), Location::Spill(s)) => {
                let offset = self.spill(s);
                self.encoder.load(d, (31, offset), (Width::Quad, false));
            }
            (Location::Spill(d), Location::General(s)) => {
                let offset = self.spill(d);
                self.encoder.store(s, (31, offset), Width::Quad);
            }
            (Location::Spill(d), Location::Spill(s)) => {
                let origin = self.spill(s);
                let target = self.spill(d);
                self.encoder.load(15, (31, origin), (Width::Quad, false));
                self.encoder.store(15, (31, target), Width::Quad);
            }
            _ => {}
        }
    }

    pub fn permute(&mut self, moves: &mut Vec<(Location, Location, Precision)>) {
        moves.retain(|&(d, s, _)| d != s);
        while !moves.is_empty() {
            let leaf = moves
                .iter()
                .position(|&(dst, _, _)| !moves.iter().any(|&(_, src, _)| src == dst));
            match leaf {
                Some(index) => {
                    let (dst, src, precision) = moves.remove(index);
                    self.glide(dst, src, precision);
                }
                None => self.detour(moves),
            }
        }
    }

    fn detour(&mut self, moves: &mut [(Location, Location, Precision)]) {
        let (_, src, precision) = moves[0];
        self.glide(Location::Float(16), src, precision);
        for item in moves.iter_mut() {
            if item.1 == src {
                item.1 = Location::Float(16);
            }
        }
    }

    fn glide(&mut self, destination: Location, source: Location, precision: Precision) {
        match (destination, source) {
            (Location::Float(d), Location::Float(s)) => self.encoder.duplicate(d, s, precision),
            (Location::Float(d), Location::Spill(s)) => {
                let offset = self.spill(s);
                self.encoder.sample(d, (31, offset), precision);
            }
            (Location::Spill(d), Location::Float(s)) => {
                let offset = self.spill(d);
                self.encoder.deposit(s, (31, offset), precision);
            }
            (Location::Spill(d), Location::Spill(s)) => {
                let origin = self.spill(s);
                let target = self.spill(d);
                self.encoder.sample(17, (31, origin), precision);
                self.encoder.deposit(17, (31, target), precision);
            }
            _ => {}
        }
    }
}
