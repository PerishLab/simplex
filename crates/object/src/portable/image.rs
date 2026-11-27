use super::format::{EXECUTABLE, SHARED, VIRTUAL, patch, store};
use super::layout::{Parts, advance, pack};
use super::survey::{Step, Survey};
use super::table;
use crate::digest::align;
use crate::{Artifact, Fault, Mode, Symbol};

struct Map {
    base: u64,
    code: u32,
    thunks: u32,
    data: u32,
    cells: u32,
    slots: Vec<u32>,
}

impl Map {
    fn symbol(&self, item: &Symbol) -> u32 {
        match item.code {
            true => self.code + item.offset as u32,
            false => self.data + item.offset as u32,
        }
    }

    fn thunk(&self, index: usize) -> u32 {
        self.thunks + (index as u32) * 8
    }

    fn cell(&self, index: usize) -> u32 {
        self.cells + (index as u32) * 8
    }
}

struct Patcher<'a> {
    code: &'a mut [u8],
    data: &'a mut [u8],
    fixes: &'a mut Vec<u32>,
}

pub struct Writer<'a> {
    artifact: &'a Artifact,
    survey: Survey,
}

impl<'a> Writer<'a> {
    pub fn new(artifact: &'a Artifact) -> Result<Self, Fault> {
        let survey = Survey::scan(artifact)?;
        Ok(Self { artifact, survey })
    }

    pub fn emit(&self) -> Result<Vec<u8>, Fault> {
        let shared = self.artifact.mode == Mode::Shared;
        let base = if shared { SHARED } else { EXECUTABLE };
        let mut code = self.artifact.code.clone();
        let jump = align(code.len() as u64, 8) as usize;
        code.resize(jump + self.survey.thunks.len() * 8, 0xCC);
        let entry = self.entrypoint(&mut code);
        let mut data = self.artifact.data.clone();
        let tail = align(data.len() as u64, 8) as usize;
        data.resize(tail + self.survey.cells.len() * 8, 0);

        let mut cursor = VIRTUAL;
        let start = cursor;
        cursor = advance(cursor, code.len().max(1));
        let storage = if data.is_empty() { 0 } else { cursor };
        if !data.is_empty() {
            cursor = advance(cursor, data.len());
        }
        let imported = match self.survey.groups.is_empty() {
            true => None,
            false => {
                let built = table::imports(cursor, &self.survey.imports, &self.survey.groups);
                cursor = advance(cursor, built.bytes.len());
                Some(built)
            }
        };
        let slots = match &imported {
            Some(built) => built.slots.clone(),
            None => Vec::new(),
        };
        let map = Map {
            base,
            code: start,
            thunks: start + jump as u32,
            data: storage,
            cells: storage + tail as u32,
            slots,
        };
        let mut fixes = Vec::new();
        let mut patcher = Patcher {
            code: &mut code,
            data: &mut data,
            fixes: &mut fixes,
        };
        self.apply(&map, &mut patcher);
        let exported = match shared {
            false => None,
            true => {
                let pairs = self.published(&map);
                let (bytes, directory) = table::exports(cursor, &self.artifact.name, &pairs);
                cursor = advance(cursor, bytes.len());
                Some((bytes, directory))
            }
        };
        let relocated = table::relocations(cursor, &fixes, shared);
        if let Some((ref bytes, _)) = relocated {
            cursor = advance(cursor, bytes.len());
        }
        Ok(pack(Parts {
            shared,
            entry: start + entry,
            base,
            image: cursor,
            code,
            data,
            imported,
            exported,
            relocated,
        }))
    }

    fn published(&self, map: &Map) -> Vec<(String, u32)> {
        let mut pairs = Vec::with_capacity(self.survey.published.len());
        for &index in &self.survey.published {
            let item = &self.artifact.symbols[index];
            pairs.push((item.name.clone(), map.symbol(item)));
        }
        pairs
    }

    fn entrypoint(&self, code: &mut Vec<u8>) -> u32 {
        if self.artifact.mode != Mode::Shared {
            return self.artifact.entry as u32;
        }
        if let Some(&index) = self.survey.definitions.get("DllMain") {
            let item = &self.artifact.symbols[index];
            if item.code {
                return item.offset as u32;
            }
        }
        let start = align(code.len() as u64, 16) as usize;
        code.resize(start, 0xCC);
        code.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3]);
        start as u32
    }

    fn apply(&self, map: &Map, out: &mut Patcher<'_>) {
        for (index, &symbol) in self.survey.cells.iter().enumerate() {
            let address = map.cell(index);
            let target = map.base + u64::from(map.symbol(&self.artifact.symbols[symbol]));
            let offset = (address - map.data) as u64;
            store(out.data, offset, target);
            out.fixes.push(address);
        }
        for (index, &import) in self.survey.thunks.iter().enumerate() {
            let origin = map.thunk(index);
            let slot = map.slots[import];
            let displacement = (i64::from(slot) - (i64::from(origin) + 6)) as i32;
            let offset = (origin - map.code) as usize;
            out.code[offset..offset + 2].copy_from_slice(&[0xFF, 0x25]);
            out.code[offset + 2..offset + 6].copy_from_slice(&displacement.to_le_bytes());
        }
        for item in &self.survey.steps {
            self.step(map, item, out);
        }
    }

    fn step(&self, map: &Map, item: &Step, out: &mut Patcher<'_>) {
        match *item {
            Step::Direct {
                offset,
                symbol,
                addend,
            } => {
                let target = i64::from(map.symbol(&self.artifact.symbols[symbol]));
                let place = i64::from(map.code) + offset as i64;
                patch(out.code, offset, (target + addend - place) as i32);
            }
            Step::Jump {
                offset,
                thunk,
                addend,
            } => {
                let target = i64::from(map.thunk(thunk));
                let place = i64::from(map.code) + offset as i64;
                patch(out.code, offset, (target + addend - place) as i32);
            }
            Step::Slot {
                offset,
                import,
                addend,
            } => {
                let target = i64::from(map.slots[import]);
                let place = i64::from(map.code) + offset as i64;
                patch(out.code, offset, (target + addend - place) as i32);
            }
            Step::Cell {
                offset,
                cell,
                addend,
            } => {
                let target = i64::from(map.cell(cell));
                let place = i64::from(map.code) + offset as i64;
                patch(out.code, offset, (target + addend - place) as i32);
            }
            Step::Pointer {
                offset,
                symbol,
                addend,
            } => {
                let rva = u64::from(map.symbol(&self.artifact.symbols[symbol]));
                let value = (map.base + rva) as i64 + addend;
                store(out.data, offset, value as u64);
                out.fixes.push(map.data + offset as u32);
            }
            Step::Bridge {
                offset,
                thunk,
                addend,
            } => {
                let rva = u64::from(map.thunk(thunk));
                let value = (map.base + rva) as i64 + addend;
                store(out.data, offset, value as u64);
                out.fixes.push(map.data + offset as u32);
            }
        }
    }
}
