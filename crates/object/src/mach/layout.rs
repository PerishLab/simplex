use super::format::{self, BASE, Section, Segment, identity};
use super::survey::Survey;
use super::trie;
use crate::digest::{align, compute as hash};
use crate::{Artifact, Mode, Symbol};

pub struct Map {
    pub base: u64,
    pub code: u64,
    pub stubs: u64,
    pub segment: u64,
    pub slots: u64,
    pub data: u64,
}

impl Map {
    pub fn symbol(&self, item: &Symbol) -> u64 {
        match item.code {
            true => self.base + self.code + item.offset,
            false => self.base + self.data + item.offset,
        }
    }

    pub fn slot(&self, index: usize) -> u64 {
        self.base + self.slots + (index as u64) * 8
    }

    pub fn stub(&self, index: usize) -> u64 {
        self.base + self.stubs + (index as u64) * 12
    }
}

pub struct Frame {
    pub base: u64,
    pub code: (u64, u64),
    pub stubs: (u64, u64),
    pub text: u64,
    pub segment: (u64, u64),
    pub slots: u64,
    pub data: (u64, u64),
    pub linkedit: (u64, u64, u64),
    pub chained: (u32, u32),
    pub exports: (u32, u32),
    pub symtab: (u32, u32),
    pub strtab: (u32, u32),
    pub counts: (u32, u32),
    pub signature: (u32, u32),
}

pub struct Planner<'a> {
    pub artifact: &'a Artifact,
    pub survey: &'a Survey,
}

impl<'a> Planner<'a> {
    pub fn exports(&self, map: &Map) -> Vec<u8> {
        let mut list = Vec::with_capacity(self.survey.published.len());
        for &index in &self.survey.published {
            let item = &self.artifact.symbols[index];
            list.push((item.name.clone(), map.symbol(item) - map.base));
        }
        trie::build(&list)
    }

    pub fn symtab(&self, map: &Map, section: u8) -> (Vec<u8>, Vec<u8>, (u32, u32)) {
        let mut locals = Vec::new();
        let mut externals = Vec::new();
        for item in &self.artifact.symbols {
            match item.exported {
                true => externals.push(item),
                false => locals.push(item),
            }
        }
        externals.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        let mut strings = vec![0u8];
        let mut symbols = Vec::with_capacity((locals.len() + externals.len()) * 16);
        for (external, group) in [(false, &locals), (true, &externals)] {
            for item in group {
                let offset = strings.len() as u32;
                strings.extend_from_slice(item.name.as_bytes());
                strings.push(0);
                let kind: u8 = if external { 0x0F } else { 0x0E };
                let index: u8 = if item.code { 1 } else { section };
                let value = map.symbol(item);
                symbols.extend_from_slice(&offset.to_le_bytes());
                symbols.push(kind);
                symbols.push(index);
                symbols.extend_from_slice(&0u16.to_le_bytes());
                symbols.extend_from_slice(&value.to_le_bytes());
            }
        }
        if symbols.is_empty() {
            let offset = strings.len() as u32;
            strings.extend_from_slice(b"_header\0");
            symbols.extend_from_slice(&offset.to_le_bytes());
            symbols.push(0x0E);
            symbols.push(1);
            symbols.extend_from_slice(&0u16.to_le_bytes());
            symbols.extend_from_slice(&(map.base + map.code).to_le_bytes());
            let padded = align(strings.len() as u64, 8) as usize;
            strings.resize(padded, 0);
            return (symbols, strings, (1, 0));
        }
        let padded = align(strings.len() as u64, 8) as usize;
        strings.resize(padded, 0);
        (
            symbols,
            strings,
            (locals.len() as u32, externals.len() as u32),
        )
    }

    pub fn measure(&self, stubs: bool, data: (bool, bool)) -> usize {
        let shared = self.artifact.mode == Mode::Shared;
        let mut total = 0usize;
        if !shared {
            total += 72;
        }
        total += 72 + (1 + usize::from(stubs)) * 80;
        let count = usize::from(data.0) + usize::from(data.1);
        if count > 0 {
            total += 72 + count * 80;
        }
        total += 72;
        if shared {
            total += format::dylib(0x0D, &identity(&self.artifact.name)).len();
        } else {
            total += format::linker("/usr/lib/dyld").len();
            total += 24;
        }
        total += 24 + 24 + 16 + 16 + 24 + 80 + 16;
        for path in &self.survey.libraries {
            total += format::dylib(0x0C, path).len();
        }
        total
    }

    pub fn commands(&self, frame: &Frame) -> (u32, Vec<u8>) {
        let shared = self.artifact.mode == Mode::Shared;
        let mut count = 0u32;
        let mut out = Vec::new();
        let mut append = |chunk: Vec<u8>| {
            count += 1;
            out.extend_from_slice(&chunk);
        };
        if !shared {
            append(format::segment(&Segment {
                name: b"__PAGEZERO",
                address: 0,
                span: BASE,
                offset: 0,
                size: 0,
                protection: 0,
                flags: 0,
                sections: Vec::new(),
            }));
        }
        append(text(frame));
        if frame.segment.1 > 0 {
            append(storage(frame));
        }
        append(format::segment(&Segment {
            name: b"__LINKEDIT",
            address: frame.base + frame.linkedit.0,
            span: frame.linkedit.2,
            offset: frame.linkedit.0,
            size: frame.linkedit.1,
            protection: 1,
            flags: 0,
            sections: Vec::new(),
        }));
        if shared {
            append(format::dylib(0x0D, &identity(&self.artifact.name)));
        } else {
            append(format::linker("/usr/lib/dyld"));
        }
        append(format::uuid(&self.fingerprint()));
        append(format::version());
        if !shared {
            append(format::main(frame.code.0 + self.artifact.entry));
        }
        for path in &self.survey.libraries {
            append(format::dylib(0x0C, path));
        }
        append(format::linkedit(
            0x8000_0034,
            frame.chained.0,
            frame.chained.1,
        ));
        append(format::linkedit(
            0x8000_0033,
            frame.exports.0,
            frame.exports.1,
        ));
        append(format::symtab(frame.symtab, frame.strtab));
        append(format::dysymtab(frame.counts.0, frame.counts.1));
        append(format::linkedit(0x1D, frame.signature.0, frame.signature.1));
        (count, out)
    }

    fn fingerprint(&self) -> [u8; 16] {
        let mut seed = Vec::with_capacity(64 + self.artifact.code.len() + self.artifact.data.len());
        seed.push(self.artifact.mode as u8);
        seed.extend_from_slice(self.artifact.name.as_bytes());
        seed.extend_from_slice(&self.artifact.entry.to_le_bytes());
        seed.extend_from_slice(&self.artifact.code);
        seed.extend_from_slice(&self.artifact.data);
        let digest = hash(&seed);
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&digest[..16]);
        uuid
    }
}

fn text(frame: &Frame) -> Vec<u8> {
    let mut sections = vec![Section {
        name: b"__text",
        segment: b"__TEXT",
        address: frame.base + frame.code.0,
        size: frame.code.1,
        offset: frame.code.0 as u32,
        align: 2,
        flags: 0x8000_0400,
        stride: 0,
    }];
    if frame.stubs.1 > 0 {
        sections.push(Section {
            name: b"__stubs",
            segment: b"__TEXT",
            address: frame.base + frame.stubs.0,
            size: frame.stubs.1,
            offset: frame.stubs.0 as u32,
            align: 2,
            flags: 0x8000_0408,
            stride: 12,
        });
    }
    format::segment(&Segment {
        name: b"__TEXT",
        address: frame.base,
        span: frame.text,
        offset: 0,
        size: frame.text,
        protection: 5,
        flags: 0,
        sections,
    })
}

fn storage(frame: &Frame) -> Vec<u8> {
    let mut sections = Vec::new();
    if frame.slots > 0 {
        sections.push(Section {
            name: b"__got",
            segment: b"__DATA",
            address: frame.base + frame.segment.0,
            size: frame.slots,
            offset: frame.segment.0 as u32,
            align: 3,
            flags: 0x06,
            stride: 0,
        });
    }
    if frame.data.1 > 0 {
        sections.push(Section {
            name: b"__data",
            segment: b"__DATA",
            address: frame.base + frame.data.0,
            size: frame.data.1,
            offset: frame.data.0 as u32,
            align: 3,
            flags: 0,
            stride: 0,
        });
    }
    format::segment(&Segment {
        name: b"__DATA",
        address: frame.base + frame.segment.0,
        span: frame.segment.1,
        offset: frame.segment.0,
        size: frame.segment.1,
        protection: 3,
        flags: 0,
        sections,
    })
}
