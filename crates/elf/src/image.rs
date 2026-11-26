use crate::dynamic::{Binding, Pool, Step, Survey, Target, hashes};
use crate::format::{self, Buffer, Head, Record, Segment, align, patch, store};
use crate::layout::{Addresses, Layout, Tables};
use crate::section::{Bundle, emit as pack};
use crate::{BASE, Fault, Mode, PAGE, Plan};

pub struct Artifact {
    pub code: Vec<u8>,
    pub data: Vec<u8>,
    pub globals: Vec<u64>,
    pub records: Vec<Record>,
}

struct Patcher<'a> {
    code: &'a mut [u8],
    data: &'a mut [u8],
    relative: &'a mut Vec<Record>,
    external: &'a mut Vec<Record>,
}

pub struct Writer<'a> {
    plan: &'a Plan,
    survey: Survey,
}

impl<'a> Writer<'a> {
    pub fn new(plan: &'a Plan) -> Result<Self, Fault> {
        let survey = Survey::scan(plan)?;
        Ok(Self { plan, survey })
    }

    pub fn emit(&self) -> Result<Vec<u8>, Fault> {
        match self.plan.mode {
            Mode::Standalone => Ok(self.standalone()),
            Mode::Dynamic | Mode::Shared => Ok(self.linked()),
        }
    }

    fn shifted(&self) -> usize {
        usize::from(self.plan.mode == Mode::Shared) * self.plan.exports.len()
    }

    fn resolve(&self, map: &Addresses) -> Artifact {
        let mut code = self.plan.code.clone();
        let mut data = self.plan.data.clone();
        let shifted = self.shifted();
        let mut globals = Vec::with_capacity(self.survey.globals.len());
        let mut relative = Vec::new();
        let mut external = Vec::new();
        for (index, target) in self.survey.globals.iter().enumerate() {
            let slot = map.globals + (index as u64) * 8;
            match *target {
                Target::Local(export) => {
                    let value = map.symbol(&self.plan.exports[export]);
                    globals.push(value);
                    relative.push(Record {
                        offset: slot,
                        symbol: 0,
                        kind: format::RELATIVE,
                        addend: value as i64,
                    });
                }
                Target::External(import) => {
                    globals.push(0);
                    external.push(Record {
                        offset: slot,
                        symbol: (1 + shifted + import) as u32,
                        kind: format::GLOBAL,
                        addend: 0,
                    });
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
        Artifact {
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
                export,
                addend,
            } => {
                let target = map.symbol(&self.plan.exports[export]) as i64;
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
                export,
                addend,
            } => {
                let value = map.symbol(&self.plan.exports[export]) as i64 + addend;
                store(out.data, offset, value as u64);
                out.relative.push(Record {
                    offset: map.data + offset,
                    symbol: 0,
                    kind: format::RELATIVE,
                    addend: value,
                });
            }
            Step::External {
                offset,
                import,
                addend,
            } => {
                store(out.data, offset, 0);
                out.external.push(Record {
                    offset: map.data + offset,
                    symbol: (1 + self.shifted() + import) as u32,
                    kind: format::DIRECT,
                    addend,
                });
            }
        }
    }

    fn standalone(&self) -> Vec<u8> {
        let plan = self.plan;
        let writable = !plan.data.is_empty() || !self.survey.globals.is_empty();
        let count: u16 = if writable { 4 } else { 3 };
        let front = 64 + 56 * u64::from(count);
        let code = align(front, PAGE);
        let data = align(code + plan.code.len() as u64, PAGE);
        let globals = align(data + plan.data.len() as u64, 8);
        let tail = globals + 8 * (self.survey.globals.len() as u64);
        let map = Addresses {
            code: BASE + code,
            stubs: 0,
            data: BASE + data,
            globals: BASE + globals,
        };
        let artifact = self.resolve(&map);
        let mut out = Buffer::new();
        out.header(&Head {
            kind: format::EXECUTABLE,
            entry: map.code + plan.entry,
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
            size: artifact.code.len() as u64,
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
        out.push(&artifact.code);
        if writable {
            out.pad(data);
            out.push(&artifact.data);
            out.pad(globals);
            for value in artifact.globals {
                out.quad(value);
            }
        }
        out.finish()
    }

    fn prepare(&self) -> Tables {
        let mut pool = Pool::new();
        let needed: Vec<u32> = self.survey.needed.iter().map(|s| pool.intern(s)).collect();
        let origin = if self.survey.needed.is_empty() {
            0
        } else {
            pool.intern("$ORIGIN")
        };
        let mut bindings = Vec::new();
        let mut exported = Vec::new();
        if self.plan.mode == Mode::Shared {
            for item in &self.plan.exports {
                exported.push(pool.intern(&item.name));
                bindings.push(Binding {
                    name: item.name.clone(),
                    defined: true,
                });
            }
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
        let layout = Layout::measure(self.plan, &self.survey, &tables);
        let artifact = self.resolve(&layout.map);
        let bundle = Bundle {
            tables,
            layout,
            artifact,
        };
        pack(self.plan, &self.survey, &bundle)
    }
}
