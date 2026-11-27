use super::format::{SYSTEM, normalize};
use crate::{Artifact, Fault, Fixup, Import, Mode, Relocation};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub enum Binding {
    Local(usize),
    External(usize),
}

pub struct Imported {
    pub name: String,
    pub ordinal: u8,
}

pub enum Step {
    Call {
        offset: u64,
        symbol: usize,
        addend: i64,
    },
    Trampoline {
        offset: u64,
        stub: usize,
        addend: i64,
    },
    Relative {
        offset: u64,
        symbol: usize,
        addend: i64,
    },
    Page {
        offset: u64,
        symbol: usize,
        addend: i64,
    },
    Upper {
        offset: u64,
        slot: usize,
    },
    Low {
        offset: u64,
        scale: u8,
        symbol: usize,
        addend: i64,
    },
    Lower {
        offset: u64,
        scale: u8,
        slot: usize,
    },
    Auto {
        offset: u64,
        slot: usize,
    },
    Rebase {
        offset: u64,
        symbol: usize,
        addend: i64,
    },
    Bind {
        offset: u64,
        import: usize,
        addend: i64,
    },
}

pub struct Survey {
    pub definitions: BTreeMap<String, usize>,
    pub published: Vec<usize>,
    pub libraries: Vec<String>,
    pub imports: Vec<Imported>,
    pub imported: BTreeMap<String, usize>,
    pub slots: Vec<Binding>,
    pub slotted: BTreeMap<String, usize>,
    pub stubs: Vec<usize>,
    pub stubbed: BTreeMap<usize, usize>,
    pub steps: Vec<Step>,
}

impl Survey {
    pub fn scan(artifact: &Artifact) -> Result<Self, Fault> {
        let mut survey = Self::prepare(artifact);
        for item in &artifact.relocations {
            survey.check(artifact, item)?;
            survey.classify(item)?;
        }
        Ok(survey)
    }

    fn prepare(artifact: &Artifact) -> Self {
        let mut definitions = BTreeMap::new();
        let mut published = Vec::new();
        for (index, item) in artifact.symbols.iter().enumerate() {
            definitions.entry(item.name.clone()).or_insert(index);
            if artifact.mode == Mode::Shared && item.exported {
                published.push(index);
            }
        }
        let mut survey = Self {
            definitions,
            published,
            libraries: vec![SYSTEM.to_owned()],
            imports: Vec::new(),
            imported: BTreeMap::new(),
            slots: Vec::new(),
            slotted: BTreeMap::new(),
            stubs: Vec::new(),
            stubbed: BTreeMap::new(),
            steps: Vec::new(),
        };
        for item in &artifact.imports {
            survey.register(item);
        }
        survey
    }

    fn register(&mut self, item: &Import) {
        if self.definitions.contains_key(&item.name) || self.imported.contains_key(&item.name) {
            return;
        }
        let path = normalize(&item.library);
        let ordinal = match self.libraries.iter().position(|entry| entry == &path) {
            Some(position) => (position + 1) as u8,
            None => {
                self.libraries.push(path);
                self.libraries.len() as u8
            }
        };
        let index = self.imports.len();
        self.imported.insert(item.name.clone(), index);
        self.imports.push(Imported {
            name: item.name.clone(),
            ordinal,
        });
    }

    fn check(&self, artifact: &Artifact, item: &Relocation) -> Result<(), Fault> {
        let (limit, width) = match item.kind {
            Fixup::Pointer => {
                if !item.offset.is_multiple_of(4) {
                    return Err(Fault::Invalid("unaligned pointer fixup in mach".into()));
                }
                (artifact.data.len() as u64, 8u64)
            }
            _ => {
                if !item.offset.is_multiple_of(4) {
                    return Err(Fault::Invalid("unaligned instruction fixup in mach".into()));
                }
                (artifact.code.len() as u64, 4u64)
            }
        };
        match item.offset.checked_add(width) {
            Some(end) if end <= limit => Ok(()),
            _ => Err(Fault::Invalid("relocation offset out of bounds".into())),
        }
    }

    fn classify(&mut self, item: &Relocation) -> Result<(), Fault> {
        let step = match item.kind {
            Fixup::Call => self.procedure(item)?,
            Fixup::Relative => self.relative(item)?,
            Fixup::Page => self.page(item)?,
            Fixup::Low { scale } => self.low(scale, item)?,
            Fixup::Got | Fixup::Global => self.global(item)?,
            Fixup::Pointer => self.pointer(item)?,
        };
        self.steps.push(step);
        Ok(())
    }

    fn procedure(&mut self, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&symbol) = self.definitions.get(&item.symbol) {
            return Ok(Step::Call {
                offset: item.offset,
                symbol,
                addend: item.addend,
            });
        }
        let Some(&import) = self.imported.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        let slot = self.slot(&item.symbol, Binding::External(import));
        let stub = *self.stubbed.entry(slot).or_insert_with(|| {
            let next = self.stubs.len();
            self.stubs.push(slot);
            next
        });
        Ok(Step::Trampoline {
            offset: item.offset,
            stub,
            addend: item.addend,
        })
    }

    fn relative(&self, item: &Relocation) -> Result<Step, Fault> {
        let Some(&symbol) = self.definitions.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        Ok(Step::Relative {
            offset: item.offset,
            symbol,
            addend: item.addend,
        })
    }

    fn page(&mut self, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&symbol) = self.definitions.get(&item.symbol) {
            return Ok(Step::Page {
                offset: item.offset,
                symbol,
                addend: item.addend,
            });
        }
        let Some(&import) = self.imported.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        let slot = self.slot(&item.symbol, Binding::External(import));
        Ok(Step::Upper {
            offset: item.offset,
            slot,
        })
    }

    fn low(&mut self, scale: u8, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&symbol) = self.definitions.get(&item.symbol) {
            return Ok(Step::Low {
                offset: item.offset,
                scale,
                symbol,
                addend: item.addend,
            });
        }
        let Some(&import) = self.imported.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        let slot = self.slot(&item.symbol, Binding::External(import));
        Ok(Step::Lower {
            offset: item.offset,
            scale,
            slot,
        })
    }

    fn global(&mut self, item: &Relocation) -> Result<Step, Fault> {
        let binding = match (
            self.definitions.get(&item.symbol),
            self.imported.get(&item.symbol),
        ) {
            (Some(&symbol), _) => Binding::Local(symbol),
            (None, Some(&import)) => Binding::External(import),
            _ => return Err(Fault::Unresolved(item.symbol.clone())),
        };
        let slot = self.slot(&item.symbol, binding);
        Ok(Step::Auto {
            offset: item.offset,
            slot,
        })
    }

    fn pointer(&mut self, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&symbol) = self.definitions.get(&item.symbol) {
            return Ok(Step::Rebase {
                offset: item.offset,
                symbol,
                addend: item.addend,
            });
        }
        let Some(&import) = self.imported.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        Ok(Step::Bind {
            offset: item.offset,
            import,
            addend: item.addend,
        })
    }

    fn slot(&mut self, name: &str, binding: Binding) -> usize {
        *self.slotted.entry(name.to_owned()).or_insert_with(|| {
            let next = self.slots.len();
            self.slots.push(binding);
            next
        })
    }
}
