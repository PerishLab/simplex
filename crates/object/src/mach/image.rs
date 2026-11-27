use super::chain::{self, Node, Payload, Topology};
use super::format::{self, BASE, Coder, PAGE};
use super::layout::{Frame, Map, Planner};
use super::sign;
use super::survey::{Binding, Step, Survey};
use crate::digest::align;
use crate::{Artifact, Fault, Mode};

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
        let base = if shared { 0 } else { BASE };
        let mut code = self.artifact.code.clone();
        if code.is_empty() {
            code.extend_from_slice(&0xD65F_03C0u32.to_le_bytes());
        }
        let bridged = !self.survey.stubs.is_empty();
        let slotted = !self.survey.slots.is_empty();
        let populated = !self.artifact.data.is_empty();
        let writable = slotted || populated;

        let planner = Planner {
            artifact: self.artifact,
            survey: &self.survey,
        };
        let span = planner.measure(bridged, (slotted, populated));
        let start = align((32 + span) as u64, 16).max(0x400);
        let jumps = align(start + code.len() as u64, 4);
        let text = align(jumps + (self.survey.stubs.len() as u64) * 12, PAGE);

        let segment = text;
        let payload = align(segment + (self.survey.slots.len() as u64) * 8, 8);
        let tail = payload + self.artifact.data.len() as u64;
        let volume = if writable {
            align((tail - segment).max(8), PAGE)
        } else {
            0
        };

        let map = Map {
            base,
            code: start,
            stubs: jumps,
            segment,
            slots: segment,
            data: payload,
        };

        let mut trampolines = Vec::with_capacity(self.survey.stubs.len() * 12);
        for (index, &slot) in self.survey.stubs.iter().enumerate() {
            let chunk = format::stub(map.stub(index), map.slot(slot))?;
            trampolines.extend_from_slice(&chunk);
        }

        let mut storage = vec![0u8; volume as usize];
        if populated {
            let mark = (payload - segment) as usize;
            storage[mark..mark + self.artifact.data.len()].copy_from_slice(&self.artifact.data);
        }
        let nodes = self.apply(&map, &mut code)?;

        let total = (if shared { 2 } else { 3 }) + usize::from(writable);
        let topology = Topology {
            segments: total,
            target: writable.then_some(if shared { 1 } else { 2 }),
            offset: segment,
            size: volume,
        };
        let chained = chain::apply(&mut storage, &nodes, &topology, &self.survey.imports)?;
        let exported = planner.exports(&map);
        let index = 1 + u8::from(bridged) + u8::from(slotted) + 1;
        let (symbols, strings, counts) = planner.symtab(&map, index);

        let linkedit = segment + volume;
        let second = linkedit + chained.len() as u64;
        let third = second + exported.len() as u64;
        let fourth = third + symbols.len() as u64;
        let fifth = align(fourth + strings.len() as u64, 16);
        let seal = sign::measure(fifth as usize, &self.artifact.name) as u64;
        let raw = (fifth + seal) - linkedit;
        let mapped = align(raw, PAGE);

        let built = planner.commands(&Frame {
            base,
            code: (start, code.len() as u64),
            stubs: (jumps, trampolines.len() as u64),
            text,
            segment: (segment, volume),
            slots: (self.survey.slots.len() as u64) * 8,
            data: (payload, self.artifact.data.len() as u64),
            linkedit: (linkedit, raw, mapped),
            chained: (linkedit as u32, chained.len() as u32),
            exports: (second as u32, exported.len() as u32),
            symtab: (third as u32, counts.0 + counts.1),
            strtab: (fourth as u32, strings.len() as u32),
            counts,
            signature: (fifth as u32, seal as u32),
        });

        let mut image = vec![0u8; fifth as usize];
        let head = format::header(shared, built.0, built.1.len() as u32);
        image[0..32].copy_from_slice(&head);
        image[32..32 + built.1.len()].copy_from_slice(&built.1);
        place(&mut image, start, &code);
        place(&mut image, jumps, &trampolines);
        if writable {
            place(&mut image, segment, &storage);
        }
        place(&mut image, linkedit, &chained);
        place(&mut image, second, &exported);
        place(&mut image, third, &symbols);
        place(&mut image, fourth, &strings);

        let signature = sign::sign(&image, &self.artifact.name, text, !shared);
        image.extend_from_slice(&signature);
        Ok(image)
    }

    fn apply(&self, map: &Map, code: &mut [u8]) -> Result<Vec<Node>, Fault> {
        let mut nodes = Vec::new();
        for (index, binding) in self.survey.slots.iter().enumerate() {
            let offset = (map.slots - map.segment) + (index as u64) * 8;
            let payload = match *binding {
                Binding::Local(symbol) => {
                    let target = map.symbol(&self.artifact.symbols[symbol]) - map.base;
                    Payload::Rebase(target)
                }
                Binding::External(import) => Payload::Bind {
                    ordinal: import as u32,
                    addend: 0,
                },
            };
            nodes.push(Node { offset, payload });
        }
        let mut coder = Coder { bytes: code };
        for item in &self.survey.steps {
            self.step(map, item, (&mut coder, &mut nodes))?;
        }
        Ok(nodes)
    }

    fn step(
        &self,
        map: &Map,
        item: &Step,
        out: (&mut Coder<'_>, &mut Vec<Node>),
    ) -> Result<(), Fault> {
        let (coder, nodes) = out;
        match *item {
            Step::Call {
                offset,
                symbol,
                addend,
            } => {
                let place = map.base + map.code + offset;
                let target = (map.symbol(&self.artifact.symbols[symbol]) as i64) + addend;
                coder.branch(offset, place, target)?;
            }
            Step::Trampoline {
                offset,
                stub,
                addend,
            } => {
                let place = map.base + map.code + offset;
                let target = (map.stub(stub) as i64) + addend;
                coder.branch(offset, place, target)?;
            }
            Step::Relative {
                offset,
                symbol,
                addend,
            } => {
                let place = map.base + map.code + offset;
                let target = (map.symbol(&self.artifact.symbols[symbol]) as i64) + addend;
                coder.relative(offset, place, target)?;
            }
            Step::Page {
                offset,
                symbol,
                addend,
            } => {
                let place = map.base + map.code + offset;
                let target = (map.symbol(&self.artifact.symbols[symbol]) as i64) + addend;
                coder.page(offset, place, target)?;
            }
            Step::Upper { offset, slot } => {
                let place = map.base + map.code + offset;
                coder.page(offset, place, map.slot(slot) as i64)?;
            }
            Step::Low {
                offset,
                scale,
                symbol,
                addend,
            } => {
                let target = (map.symbol(&self.artifact.symbols[symbol]) as i64) + addend;
                coder.low(offset, scale, target)?;
            }
            Step::Lower {
                offset,
                scale,
                slot,
            } => {
                coder.low(offset, scale, map.slot(slot) as i64)?;
            }
            Step::Auto { offset, slot } => {
                let target = map.slot(slot) as i64;
                let place = map.base + map.code + offset;
                match coder.paged(offset) {
                    true => coder.page(offset, place, target)?,
                    false => coder.low(offset, 3, target)?,
                }
            }
            Step::Rebase {
                offset,
                symbol,
                addend,
            } => {
                let location = (map.data - map.segment) + offset;
                let value = ((map.symbol(&self.artifact.symbols[symbol]) as i64) + addend) as u64;
                nodes.push(Node {
                    offset: location,
                    payload: Payload::Rebase(value - map.base),
                });
            }
            Step::Bind {
                offset,
                import,
                addend,
            } => {
                let location = (map.data - map.segment) + offset;
                nodes.push(Node {
                    offset: location,
                    payload: Payload::Bind {
                        ordinal: import as u32,
                        addend,
                    },
                });
            }
        }
        Ok(())
    }
}

fn place(image: &mut [u8], offset: u64, slice: &[u8]) {
    let start = offset as usize;
    image[start..start + slice.len()].copy_from_slice(slice);
}
