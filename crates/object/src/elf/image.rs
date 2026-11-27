use super::dynamic::{Binding, Pool, Step, Survey, Target, hashes};
use super::format::{self, Buffer, Entry, Head, Segment, patch, store};
use super::layout::{Addresses, Layout, Tables};
use super::section::{Bundle, emit as pack};
use super::{BASE, PAGE};
use crate::digest::align;
use crate::{Artifact, Fault, Mode};

pub struct Resolved {
    pub code: Vec<u8>,
    pub data: Vec<u8>,
    pub globals: Vec<u64>,
    pub records: Vec<Entry>,
}

struct Patcher<'a> {
    code: &'a mut [u8],
    data: &'a mut [u8],
    relative: &'a mut Vec<Entry>,
    external: &'a mut Vec<Entry>,
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
        match self.artifact.mode {
            Mode::Standalone => Ok(self.standalone()),
            Mode::Dynamic | Mode::Shared => Ok(self.linked()),
        }
    }

    fn shifted(&self) -> usize {
        self.survey.published.len()
    }

    fn resolve(&self, map: &Addresses) -> Resolved {
        let mut code = self.artifact.code.clone();
        let mut data = self.artifact.data.clone();
        let shifted = self.shifted();
        let mut globals = Vec::with_capacity(self.survey.globals.len());
        let mut relative = Vec::new();
        let mut external = Vec::new();
        for (index, target) in self.survey.globals.iter().enumerate() {
            let slot = map.globals + (index as u64) * 8;
            match *target {
                Target::Local(symbol) => {
                    let value = map.symbol(&self.artifact.symbols[symbol]);
                    globals.push(value);
                    relative.push(Entry::pack(slot, 0, format::RELATIVE, value as i64));
                }
                Target::External(import) => {
                    globals.push(0);
                    let index = (1 + shifted + import) as u32;
                    external.push(Entry::pack(slot, index, format::GLOBAL, 0));
                }
            }
        }
        let mut patcher = Patcher {
            code: &mut code,
            data: &mut data,
            relative: &mut relative,
            external: &mut external,
        };
        for step in &self.survey.steps {
            self.apply(map, step, &mut patcher);
        }
        relative.extend(external);
        Resolved {
            code,
            data,
            globals,
            records: relative,
        }
    }

    fn apply(&self, map: &Addresses, step: &Step, out: &mut Patcher<'_>) {
        match *step {
            Step::Direct {
                offset,
                symbol,
                addend,
            } => {
                let target = map.symbol(&self.artifact.symbols[symbol]) as i64;
                let place = (map.code + offset) as i64;
                patch(out.code, offset, (target + addend - place) as i32);
            }
            Step::Call {
                offset,
                slot,
                addend,
            } => {
                let target = (map.stubs + 16 * (1 + slot as u64)) as i64;
                let place = (map.code + offset) as i64;
                patch(out.code, offset, (target + addend - place) as i32);
            }
            Step::Load {
                offset,
                slot,
                addend,
            } => {
                let target = (map.globals + 8 * (slot as u64)) as i64;
                let place = (map.code + offset) as i64;
                patch(out.code, offset, (target + addend - place) as i32);
            }
            Step::Pointer {
                offset,
                symbol,
                addend,
            } => {
                let value = map.symbol(&self.artifact.symbols[symbol]) as i64 + addend;
                store(out.data, offset, value as u64);
                out.relative
                    .push(Entry::pack(map.data + offset, 0, format::RELATIVE, value));
            }
            Step::External {
                offset,
                import,
                addend,
            } => {
                store(out.data, offset, 0);
                let index = (1 + self.shifted() + import) as u32;
                out.external.push(Entry::pack(
                    map.data + offset,
                    index,
                    format::DIRECT,
                    addend,
                ));
            }
        }
    }

    fn standalone(&self) -> Vec<u8> {
        let artifact = self.artifact;
        let writable = !artifact.data.is_empty() || !self.survey.globals.is_empty();
        let count: u16 = if writable { 4 } else { 3 };
        let front = 64 + 56 * u64::from(count);
        let code = align(front, PAGE);
        let data = align(code + artifact.code.len() as u64, PAGE);
        let globals = align(data + artifact.data.len() as u64, 8);
        let tail = globals + 8 * (self.survey.globals.len() as u64);
        let map = Addresses {
            code: BASE + code,
            stubs: 0,
            data: BASE + data,
            globals: BASE + globals,
        };
        let resolved = self.resolve(&map);
        let mut out = Buffer::new();
        out.header(&Head {
            kind: format::EXECUTABLE,
            entry: map.code + artifact.entry,
            count,
        });
        out.segment(&Segment {
            kind: format::LOAD,
            flags: format::READ,
            offset: 0,
            address: BASE,
            size: front,
            align: PAGE,
        });
        out.segment(&Segment {
            kind: format::LOAD,
            flags: format::READ | format::EXECUTE,
            offset: code,
            address: map.code,
            size: resolved.code.len() as u64,
            align: PAGE,
        });
        if writable {
            out.segment(&Segment {
                kind: format::LOAD,
                flags: format::READ | format::WRITE,
                offset: data,
                address: map.data,
                size: tail - data,
                align: PAGE,
            });
        }
        out.segment(&Segment {
            kind: format::STACK,
            flags: format::READ | format::WRITE,
            offset: 0,
            address: 0,
            size: 0,
            align: 16,
        });
        out.pad(code);
        out.push(&resolved.code);
        if writable {
            out.pad(data);
            out.push(&resolved.data);
            out.pad(globals);
            for value in resolved.globals {
                out.quad(value);
            }
        }
        out.finish()
    }

    fn prepare(&self) -> Tables {
        let mut pool = Pool::new();
        let needed: Vec<u32> = self.survey.needed.iter().map(|s| pool.intern(s)).collect();
        let origin = match self.survey.needed.is_empty() {
            true => 0,
            false => pool.intern("$ORIGIN"),
        };
        let mut bindings = Vec::new();
        let mut exported = Vec::new();
        for &index in &self.survey.published {
            let item = &self.artifact.symbols[index];
            exported.push(pool.intern(&item.name));
            bindings.push(Binding {
                name: item.name.clone(),
                defined: true,
            });
        }
        let mut imported = Vec::new();
        for name in &self.survey.imports {
            imported.push(pool.intern(name));
            bindings.push(Binding {
                name: name.clone(),
                defined: false,
            });
        }
        let hash = hashes(&bindings);
        Tables {
            pool,
            needed,
            origin,
            exported,
            imported,
            hash,
        }
    }

    fn linked(&self) -> Vec<u8> {
        let tables = self.prepare();
        let layout = Layout::measure(self.artifact, &self.survey, &tables);
        let resolved = self.resolve(&layout.map);
        let bundle = Bundle {
            tables,
            layout,
            resolved,
        };
        pack(self.artifact, &self.survey, &bundle)
    }
}
