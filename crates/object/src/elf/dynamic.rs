use crate::{Artifact, Fault, Fixup, Mode, Relocation};
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

pub fn scramble(name: &[u8]) -> u32 {
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
            let slot = (scramble(item.name.as_bytes()) as usize) % count;
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
        symbol: usize,
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
        symbol: usize,
        addend: i64,
    },
    External {
        offset: u64,
        import: usize,
        addend: i64,
    },
}

pub struct Survey {
    pub definitions: BTreeMap<String, usize>,
    pub published: Vec<usize>,
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
    pub fn scan(artifact: &Artifact) -> Result<Self, Fault> {
        let mut survey = Self::empty(artifact);
        for item in &artifact.relocations {
            survey.check(artifact, item)?;
            survey.classify(artifact.mode, item)?;
        }
        Ok(survey)
    }

    fn empty(artifact: &Artifact) -> Self {
        let mut definitions = BTreeMap::new();
        let mut published = Vec::new();
        for (index, item) in artifact.symbols.iter().enumerate() {
            definitions.entry(item.name.clone()).or_insert(index);
            if artifact.mode == Mode::Shared && item.exported {
                published.push(index);
            }
        }
        let mut imports = Vec::new();
        let mut imported = BTreeMap::new();
        let mut needed = Vec::new();
        for item in &artifact.imports {
            if !item.library.is_empty() && !needed.contains(&item.library) {
                needed.push(item.library.clone());
            }
            if !definitions.contains_key(&item.name) && !imported.contains_key(&item.name) {
                imported.insert(item.name.clone(), imports.len());
                imports.push(item.name.clone());
            }
        }
        Self {
            definitions,
            published,
            imports,
            imported,
            needed,
            procedures: Vec::new(),
            procedure: BTreeMap::new(),
            globals: Vec::new(),
            global: BTreeMap::new(),
            steps: Vec::new(),
            pointers: 0,
            externals: 0,
        }
    }

    fn check(&self, artifact: &Artifact, item: &Relocation) -> Result<(), Fault> {
        let (limit, width) = match item.kind {
            Fixup::Pointer => (artifact.data.len() as u64, 8u64),
            Fixup::Page | Fixup::Low { .. } => {
                return Err(Fault::Invalid("page relocation unsupported in elf".into()));
            }
            _ => (artifact.code.len() as u64, 4u64),
        };
        match item.offset.checked_add(width) {
            Some(end) if end <= limit => Ok(()),
            _ => Err(Fault::Invalid("relocation offset out of bounds".into())),
        }
    }

    fn classify(&mut self, mode: Mode, item: &Relocation) -> Result<(), Fault> {
        let step = match item.kind {
            Fixup::Relative => self.relative(item)?,
            Fixup::Call => self.procedure(mode, item)?,
            Fixup::Got | Fixup::Global => self.global(mode, item)?,
            Fixup::Pointer => self.absolute(mode, item)?,
            Fixup::Page | Fixup::Low { .. } => {
                return Err(Fault::Invalid("page relocation unsupported in elf".into()));
            }
        };
        self.steps.push(step);
        Ok(())
    }

    fn relative(&self, item: &Relocation) -> Result<Step, Fault> {
        let Some(&symbol) = self.definitions.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        Ok(Step::Direct {
            offset: item.offset,
            symbol,
            addend: item.addend,
        })
    }

    fn procedure(&mut self, mode: Mode, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&symbol) = self.definitions.get(&item.symbol) {
            return Ok(Step::Direct {
                offset: item.offset,
                symbol,
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
            self.definitions.get(&item.symbol),
            self.imported.get(&item.symbol),
        ) {
            (Some(&symbol), _) => Target::Local(symbol),
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
        if let Some(&symbol) = self.definitions.get(&item.symbol) {
            self.pointers += 1;
            return Ok(Step::Pointer {
                offset: item.offset,
                symbol,
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
