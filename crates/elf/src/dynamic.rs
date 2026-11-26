use crate::{Fault, Kind, Mode, Plan, Relocation};
use std::collections::BTreeMap;

pub struct Pool {
    bytes: Vec<u8>,
    offsets: BTreeMap<String, u32>,
}

impl Pool {
    pub fn new() -> Self {
        Self {
            bytes: vec![0],
            offsets: BTreeMap::new(),
        }
    }

    pub fn intern(&mut self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        if let Some(&offset) = self.offsets.get(text) {
            return offset;
        }
        let offset = self.bytes.len() as u32;
        self.bytes.extend_from_slice(text.as_bytes());
        self.bytes.push(0);
        self.offsets.insert(text.to_owned(), offset);
        offset
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn size(&self) -> u64 {
        self.bytes.len() as u64
    }
}

pub struct Binding {
    pub name: String,
    pub defined: bool,
}

pub fn digest(name: &[u8]) -> u32 {
    let mut state: u32 = 0;
    for &byte in name {
        state = (state << 4).wrapping_add(u32::from(byte));
        let top = state & 0xf000_0000;
        if top != 0 {
            state ^= top >> 24;
        }
        state &= !top;
    }
    state
}

pub fn hashes(bindings: &[Binding]) -> Vec<u8> {
    let count = bindings.len().max(1);
    let total = bindings.len() + 1;
    let mut buckets = vec![0u32; count];
    let mut chains = vec![0u32; total];
    for (position, item) in bindings.iter().enumerate() {
        if item.defined {
            let index = (position + 1) as u32;
            let slot = (digest(item.name.as_bytes()) as usize) % count;
            chains[index as usize] = buckets[slot];
            buckets[slot] = index;
        }
    }
    let mut out = Vec::with_capacity((2 + count + total) * 4);
    out.extend_from_slice(&(count as u32).to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    for value in buckets.into_iter().chain(chains) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

#[derive(Clone, Copy)]
pub enum Target {
    Local(usize),
    External(usize),
}

pub enum Step {
    Direct {
        offset: u64,
        export: usize,
        addend: i64,
    },
    Call {
        offset: u64,
        slot: usize,
        addend: i64,
    },
    Load {
        offset: u64,
        slot: usize,
        addend: i64,
    },
    Pointer {
        offset: u64,
        export: usize,
        addend: i64,
    },
    External {
        offset: u64,
        import: usize,
        addend: i64,
    },
}

pub struct Survey {
    pub exports: BTreeMap<String, usize>,
    pub imports: Vec<String>,
    pub imported: BTreeMap<String, usize>,
    pub needed: Vec<String>,
    pub procedures: Vec<String>,
    pub procedure: BTreeMap<String, usize>,
    pub globals: Vec<Target>,
    pub global: BTreeMap<String, usize>,
    pub steps: Vec<Step>,
    pub pointers: usize,
    pub externals: usize,
}

impl Survey {
    pub fn scan(plan: &Plan) -> Result<Self, Fault> {
        let mut survey = Self::empty(plan);
        for item in &plan.relocations {
            survey.classify(plan.mode, item)?;
        }
        Ok(survey)
    }

    fn empty(plan: &Plan) -> Self {
        let mut exports = BTreeMap::new();
        for (index, item) in plan.exports.iter().enumerate() {
            exports.entry(item.name.clone()).or_insert(index);
        }
        let mut imports = Vec::new();
        let mut imported = BTreeMap::new();
        for name in &plan.imports {
            if !exports.contains_key(name) && !imported.contains_key(name) {
                imported.insert(name.clone(), imports.len());
                imports.push(name.clone());
            }
        }
        Self {
            exports,
            imports,
            imported,
            needed: unique(&plan.needed),
            procedures: Vec::new(),
            procedure: BTreeMap::new(),
            globals: Vec::new(),
            global: BTreeMap::new(),
            steps: Vec::new(),
            pointers: 0,
            externals: 0,
        }
    }

    fn classify(&mut self, mode: Mode, item: &Relocation) -> Result<(), Fault> {
        let step = match item.kind {
            Kind::Relative32 => self.relative(item)?,
            Kind::Procedure32 => self.procedure(mode, item)?,
            Kind::Global32 => self.global(mode, item)?,
            Kind::Absolute64 => self.absolute(mode, item)?,
        };
        self.steps.push(step);
        Ok(())
    }

    fn relative(&self, item: &Relocation) -> Result<Step, Fault> {
        let Some(&export) = self.exports.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        Ok(Step::Direct {
            offset: item.offset,
            export,
            addend: item.addend,
        })
    }

    fn procedure(&mut self, mode: Mode, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&export) = self.exports.get(&item.symbol) {
            return Ok(Step::Direct {
                offset: item.offset,
                export,
                addend: item.addend,
            });
        }
        if mode == Mode::Standalone || !self.imported.contains_key(&item.symbol) {
            return Err(Fault::Unresolved(item.symbol.clone()));
        }
        let slot = *self
            .procedure
            .entry(item.symbol.clone())
            .or_insert_with(|| {
                let next = self.procedures.len();
                self.procedures.push(item.symbol.clone());
                next
            });
        Ok(Step::Call {
            offset: item.offset,
            slot,
            addend: item.addend,
        })
    }

    fn global(&mut self, mode: Mode, item: &Relocation) -> Result<Step, Fault> {
        let target = match (
            self.exports.get(&item.symbol),
            self.imported.get(&item.symbol),
        ) {
            (Some(&export), _) => Target::Local(export),
            (None, Some(&import)) if mode != Mode::Standalone => Target::External(import),
            _ => return Err(Fault::Unresolved(item.symbol.clone())),
        };
        let slot = *self.global.entry(item.symbol.clone()).or_insert_with(|| {
            let next = self.globals.len();
            self.globals.push(target);
            next
        });
        Ok(Step::Load {
            offset: item.offset,
            slot,
            addend: item.addend,
        })
    }

    fn absolute(&mut self, mode: Mode, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&export) = self.exports.get(&item.symbol) {
            self.pointers += 1;
            return Ok(Step::Pointer {
                offset: item.offset,
                export,
                addend: item.addend,
            });
        }
        let Some(&import) = self.imported.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        if mode == Mode::Standalone {
            return Err(Fault::Unresolved(item.symbol.clone()));
        }
        self.externals += 1;
        Ok(Step::External {
            offset: item.offset,
            import,
            addend: item.addend,
        })
    }

    pub fn records(&self) -> usize {
        self.globals.len() + self.pointers + self.externals
    }
}

fn unique(items: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        if !out.contains(item) {
            out.push(item.clone());
        }
    }
    out
}
