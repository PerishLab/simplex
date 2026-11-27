use crate::machine::{Bank, Location, Span};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub locations: BTreeMap<u32, Location>,
    pub used: Vec<u8>,
    pub frame: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    General(u8),
    Float(u8),
}

#[derive(Clone, Copy)]
struct Active {
    end: u32,
    value: u32,
    choice: Choice,
}

struct Allocator<'a> {
    bank: &'a Bank,
    chosen: BTreeMap<u32, Choice>,
    spilled: BTreeSet<u32>,
    active: Vec<Active>,
}

impl<'a> Allocator<'a> {
    fn new(bank: &'a Bank) -> Self {
        Self {
            bank,
            chosen: BTreeMap::new(),
            spilled: BTreeSet::new(),
            active: Vec::new(),
        }
    }

    fn candidates(&self, item: &Span) -> Vec<u8> {
        match (item.float, item.across) {
            (true, true) => Vec::new(),
            (true, false) => self.bank.float.clone(),
            (false, true) => self.bank.preserved.clone(),
            (false, false) => self.general(),
        }
    }

    fn general(&self) -> Vec<u8> {
        let mut list: Vec<u8> = self
            .bank
            .general
            .iter()
            .copied()
            .filter(|reg| !self.bank.preserved.contains(reg))
            .collect();
        for &reg in &self.bank.preserved {
            list.push(reg);
        }
        list.dedup();
        list
    }

    fn step(&mut self, item: &Span) {
        self.active.retain(|entry| entry.end >= item.start);
        let pool = self.candidates(item);
        if pool.is_empty() {
            self.spilled.insert(item.value);
            return;
        }
        let free = pool.iter().copied().find(|&reg| {
            let target = wrap(item.float, reg);
            !self.active.iter().any(|entry| entry.choice == target)
        });
        match free {
            Some(reg) => self.record(item, wrap(item.float, reg)),
            None => self.evict(&pool, item),
        }
    }

    fn record(&mut self, item: &Span, choice: Choice) {
        self.chosen.insert(item.value, choice);
        self.active.push(Active {
            end: item.end,
            value: item.value,
            choice,
        });
    }

    fn evict(&mut self, pool: &[u8], item: &Span) {
        let victim = self
            .active
            .iter()
            .enumerate()
            .filter(|(_, entry)| match (item.float, entry.choice) {
                (true, Choice::Float(reg)) => pool.contains(&reg),
                (false, Choice::General(reg)) => pool.contains(&reg),
                _ => false,
            })
            .max_by_key(|(_, entry)| entry.end);
        match victim {
            Some((pos, entry)) if entry.end > item.end => {
                let removed = self.active.remove(pos);
                self.chosen.remove(&removed.value);
                self.spilled.insert(removed.value);
                self.record(item, removed.choice);
            }
            _ => {
                self.spilled.insert(item.value);
            }
        }
    }

    fn finish(self, spans: &[Span]) -> Plan {
        let (offsets, frame) = slots(spans, &self.spilled);
        let mut locations = BTreeMap::new();
        for item in spans {
            let loc = match self.chosen.get(&item.value).copied() {
                Some(Choice::General(reg)) => Location::General(reg),
                Some(Choice::Float(reg)) => Location::Float(reg),
                None => Location::Spill(offsets.get(&item.value).copied().unwrap_or(0)),
            };
            locations.insert(item.value, loc);
        }
        let used = self
            .bank
            .preserved
            .iter()
            .copied()
            .filter(|&reg| locations.values().any(|&loc| loc == Location::General(reg)))
            .collect();
        Plan {
            locations,
            used,
            frame,
        }
    }
}

pub fn assign(bank: &Bank, spans: &[Span]) -> Plan {
    let ordered = normalize(spans);
    let mut allocator = Allocator::new(bank);
    for item in &ordered {
        allocator.step(item);
    }
    allocator.finish(&ordered)
}

fn wrap(float: bool, reg: u8) -> Choice {
    match float {
        true => Choice::Float(reg),
        false => Choice::General(reg),
    }
}

fn normalize(spans: &[Span]) -> Vec<Span> {
    let mut merged: BTreeMap<u32, Span> = BTreeMap::new();
    for &item in spans {
        merged
            .entry(item.value)
            .and_modify(|prev| {
                prev.start = prev.start.min(item.start);
                prev.end = prev.end.max(item.end);
                prev.float |= item.float;
                prev.across |= item.across;
            })
            .or_insert(item);
    }
    let mut list: Vec<Span> = merged.into_values().collect();
    list.sort_by_key(|item| (item.start, item.end, item.value));
    list
}

fn slots(spans: &[Span], spilled: &BTreeSet<u32>) -> (BTreeMap<u32, i32>, u32) {
    let mut ends: Vec<u32> = Vec::new();
    let mut offsets = BTreeMap::new();
    for item in spans {
        if !spilled.contains(&item.value) {
            continue;
        }
        let index = match ends.iter().position(|&end| end < item.start) {
            Some(idx) => {
                ends[idx] = item.end;
                idx
            }
            None => {
                let idx = ends.len();
                ends.push(item.end);
                idx
            }
        };
        offsets.insert(item.value, (index * 8) as i32);
    }
    let count = ends.len() as u32;
    let frame = match count {
        0 => 0,
        n => (n * 8 + 15) & !15,
    };
    (offsets, frame)
}
