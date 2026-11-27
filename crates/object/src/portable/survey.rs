use crate::{Artifact, Fault, Fixup, Import, Mode, Relocation};
use std::collections::BTreeMap;

pub struct Group {
    pub library: String,
    pub members: Vec<usize>,
}

pub enum Step {
    Direct {
        offset: u64,
        symbol: usize,
        addend: i64,
    },
    Jump {
        offset: u64,
        thunk: usize,
        addend: i64,
    },
    Slot {
        offset: u64,
        import: usize,
        addend: i64,
    },
    Cell {
        offset: u64,
        cell: usize,
        addend: i64,
    },
    Pointer {
        offset: u64,
        symbol: usize,
        addend: i64,
    },
    Bridge {
        offset: u64,
        thunk: usize,
        addend: i64,
    },
}

pub struct Survey {
    pub definitions: BTreeMap<String, usize>,
    pub published: Vec<usize>,
    pub imports: Vec<Import>,
    pub imported: BTreeMap<String, usize>,
    pub groups: Vec<Group>,
    pub thunks: Vec<usize>,
    pub thunked: BTreeMap<usize, usize>,
    pub cells: Vec<usize>,
    pub celled: BTreeMap<usize, usize>,
    pub steps: Vec<Step>,
}

impl Survey {
    pub fn scan(artifact: &Artifact) -> Result<Self, Fault> {
        let mut survey = Self::prepare(artifact)?;
        for item in &artifact.relocations {
            survey.check(artifact, item)?;
            survey.classify(item)?;
        }
        Ok(survey)
    }

    fn prepare(artifact: &Artifact) -> Result<Self, Fault> {
        let mut definitions = BTreeMap::new();
        let mut published = Vec::new();
        for (index, item) in artifact.symbols.iter().enumerate() {
            definitions.entry(item.name.clone()).or_insert(index);
            if artifact.mode == Mode::Shared && item.exported {
                published.push(index);
            }
        }
        published.sort_by(|&left, &right| {
            let first = artifact.symbols[left].name.as_bytes();
            let second = artifact.symbols[right].name.as_bytes();
            first.cmp(second)
        });
        let mut survey = Self {
            definitions,
            published,
            imports: Vec::new(),
            imported: BTreeMap::new(),
            groups: Vec::new(),
            thunks: Vec::new(),
            thunked: BTreeMap::new(),
            cells: Vec::new(),
            celled: BTreeMap::new(),
            steps: Vec::new(),
        };
        for item in &artifact.imports {
            survey.register(item)?;
        }
        Ok(survey)
    }

    fn register(&mut self, item: &Import) -> Result<(), Fault> {
        if self.definitions.contains_key(&item.name) || self.imported.contains_key(&item.name) {
            return Ok(());
        }
        if item.library.is_empty() {
            return Err(Fault::Invalid("portable import missing library".into()));
        }
        let index = self.imports.len();
        self.imported.insert(item.name.clone(), index);
        self.imports.push(item.clone());
        if let Some(group) = self.groups.iter_mut().find(|g| g.library == item.library) {
            group.members.push(index);
            return Ok(());
        }
        self.groups.push(Group {
            library: item.library.clone(),
            members: vec![index],
        });
        Ok(())
    }

    fn check(&self, artifact: &Artifact, item: &Relocation) -> Result<(), Fault> {
        let (limit, width) = match item.kind {
            Fixup::Pointer => (artifact.data.len() as u64, 8u64),
            Fixup::Page | Fixup::Low { .. } => {
                return Err(Fault::Invalid(
                    "page relocation unsupported in portable".into(),
                ));
            }
            _ => (artifact.code.len() as u64, 4u64),
        };
        match item.offset.checked_add(width) {
            Some(end) if end <= limit => Ok(()),
            _ => Err(Fault::Invalid("relocation offset out of bounds".into())),
        }
    }

    fn classify(&mut self, item: &Relocation) -> Result<(), Fault> {
        let step = match item.kind {
            Fixup::Relative => self.relative(item)?,
            Fixup::Call => self.procedure(item)?,
            Fixup::Got | Fixup::Global => self.global(item)?,
            Fixup::Pointer => self.pointer(item)?,
            Fixup::Page | Fixup::Low { .. } => {
                return Err(Fault::Invalid(
                    "page relocation unsupported in portable".into(),
                ));
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

    fn procedure(&mut self, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&symbol) = self.definitions.get(&item.symbol) {
            return Ok(Step::Direct {
                offset: item.offset,
                symbol,
                addend: item.addend,
            });
        }
        let Some(&import) = self.imported.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        let thunk = self.thunk(import);
        Ok(Step::Jump {
            offset: item.offset,
            thunk,
            addend: item.addend,
        })
    }

    fn global(&mut self, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&import) = self.imported.get(&item.symbol) {
            return Ok(Step::Slot {
                offset: item.offset,
                import,
                addend: item.addend,
            });
        }
        let Some(&symbol) = self.definitions.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        let cell = *self.celled.entry(symbol).or_insert_with(|| {
            let next = self.cells.len();
            self.cells.push(symbol);
            next
        });
        Ok(Step::Cell {
            offset: item.offset,
            cell,
            addend: item.addend,
        })
    }

    fn pointer(&mut self, item: &Relocation) -> Result<Step, Fault> {
        if let Some(&symbol) = self.definitions.get(&item.symbol) {
            return Ok(Step::Pointer {
                offset: item.offset,
                symbol,
                addend: item.addend,
            });
        }
        let Some(&import) = self.imported.get(&item.symbol) else {
            return Err(Fault::Unresolved(item.symbol.clone()));
        };
        if !self.imports[import].code {
            return Err(Fault::Invalid(
                "portable data import relocation unsupported".into(),
            ));
        }
        let thunk = self.thunk(import);
        Ok(Step::Bridge {
            offset: item.offset,
            thunk,
            addend: item.addend,
        })
    }

    fn thunk(&mut self, import: usize) -> usize {
        *self.thunked.entry(import).or_insert_with(|| {
            let next = self.thunks.len();
            self.thunks.push(import);
            next
        })
    }
}
