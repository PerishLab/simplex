use crate::dynamic::{Pool, Survey};
use crate::format::{self, align};
use crate::{INTERPRETER, Mode, PAGE, Plan, Symbol};

pub struct Addresses {
    pub code: u64,
    pub stubs: u64,
    pub data: u64,
    pub globals: u64,
}

impl Addresses {
    pub fn symbol(&self, item: &Symbol) -> u64 {
        match item.code {
            true => self.code + item.offset,
            false => self.data + item.offset,
        }
    }
}

pub struct Tables {
    pub pool: Pool,
    pub needed: Vec<u32>,
    pub origin: u32,
    pub exported: Vec<u32>,
    pub imported: Vec<u32>,
    pub hash: Vec<u8>,
}

pub struct Layout {
    pub count: u16,
    pub interp: u64,
    pub hash: u64,
    pub symbols: u64,
    pub strings: u64,
    pub records: u64,
    pub jumps: u64,
    pub front: u64,
    pub executable: bool,
    pub map: Addresses,
    pub span: u64,
    pub linkage: u64,
    pub dynamic: u64,
    pub pairs: Vec<(i64, u64)>,
    pub tags: u64,
    pub volume: u64,
}

impl Layout {
    pub fn measure(plan: &Plan, survey: &Survey, tables: &Tables) -> Self {
        let executable = !plan.code.is_empty() || !survey.procedures.is_empty();
        let count: u16 = match (plan.mode, executable) {
            (Mode::Dynamic, _) => 7,
            (_, true) => 6,
            (_, false) => 5,
        };
        let mut cursor = 64 + 56 * u64::from(count);
        let interp = cursor;
        if plan.mode == Mode::Dynamic {
            cursor += INTERPRETER.len() as u64 + 1;
        }
        let hash = align(cursor, 4);
        cursor = hash + tables.hash.len() as u64;
        let symbols = align(cursor, 8);
        let entries = 1 + tables.exported.len() + tables.imported.len();
        cursor = symbols + 24 * (entries as u64);
        let strings = cursor;
        cursor = strings + tables.pool.size();
        let records = align(cursor, 8);
        cursor = records + 24 * (survey.records() as u64);
        let jumps = align(cursor, 8);
        cursor = jumps + 24 * (survey.procedures.len() as u64);
        let front = cursor;
        let code = align(front, PAGE);
        let mut stubs = code;
        let mut span = 0;
        if executable {
            cursor = code + plan.code.len() as u64;
            if !survey.procedures.is_empty() {
                stubs = align(cursor, 16);
                cursor = stubs + 16 * (1 + survey.procedures.len() as u64);
            }
            span = cursor - code;
        }
        let data = align(cursor, PAGE);
        cursor = data + plan.data.len() as u64;
        let globals = align(cursor, 8);
        cursor = globals + 8 * (survey.globals.len() as u64);
        let linkage = align(cursor, 8);
        if !survey.procedures.is_empty() {
            cursor = linkage + 8 * (3 + survey.procedures.len() as u64);
        }
        let dynamic = align(cursor, 8);
        let mut out = Self {
            count,
            interp,
            hash,
            symbols,
            strings,
            records,
            jumps,
            front,
            executable,
            map: Addresses {
                code,
                stubs,
                data,
                globals,
            },
            span,
            linkage,
            dynamic,
            pairs: Vec::new(),
            tags: 0,
            volume: 0,
        };
        out.pairs = out.collect(plan.mode, survey, tables);
        out.tags = 16 * (out.pairs.len() as u64);
        out.volume = (out.dynamic + out.tags) - out.map.data;
        out
    }

    fn collect(&self, mode: Mode, survey: &Survey, tables: &Tables) -> Vec<(i64, u64)> {
        let mut list = Vec::new();
        for &name in &tables.needed {
            list.push((format::NEEDED, u64::from(name)));
        }
        if !tables.needed.is_empty() {
            list.push((format::SEARCH, u64::from(tables.origin)));
        }
        list.push((format::HASH, self.hash));
        list.push((format::STRINGS, self.strings));
        list.push((format::SYMBOLS, self.symbols));
        list.push((format::LENGTH, tables.pool.size()));
        list.push((format::UNIT, 24));
        let count = survey.records() as u64;
        if count > 0 {
            list.push((format::RECORDS, self.records));
            list.push((format::SPAN, count * 24));
            list.push((format::STRIDE, 24));
        }
        let calls = survey.procedures.len() as u64;
        if calls > 0 {
            list.push((format::TABLE, self.linkage));
            list.push((format::FORMAT, format::RECORDS as u64));
            list.push((format::TOTAL, calls * 24));
            list.push((format::JUMPS, self.jumps));
        }
        if mode == Mode::Dynamic {
            list.push((format::STATE, format::POSITION));
        }
        list.push((0, 0));
        list
    }
}
