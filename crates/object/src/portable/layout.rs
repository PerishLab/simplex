use super::format::{
    CODE, DISCARDABLE, Directory, Envelope, READABLE, SECTOR, Section, VIRTUAL, WRITABLE, headers,
    label, pad,
};
use super::table::Imported;
use crate::digest::align;

pub struct Parts {
    pub shared: bool,
    pub entry: u32,
    pub base: u64,
    pub image: u32,
    pub code: Vec<u8>,
    pub data: Vec<u8>,
    pub imported: Option<Imported>,
    pub exported: Option<(Vec<u8>, Directory)>,
    pub relocated: Option<(Vec<u8>, Directory)>,
}

pub fn advance(address: u32, length: usize) -> u32 {
    let span = length.max(1) as u64;
    address + align(span, u64::from(VIRTUAL)) as u32
}

pub fn pack(parts: Parts) -> Vec<u8> {
    let mut directories = [Directory::default(); 16];
    let mut chunks: Vec<([u8; 8], u32, Vec<u8>)> = Vec::new();
    chunks.push((label(b".text"), CODE, parts.code));
    if !parts.data.is_empty() {
        chunks.push((label(b".data"), WRITABLE, parts.data));
    }
    if let Some(built) = parts.imported {
        directories[1] = built.table;
        directories[12] = built.addresses;
        chunks.push((label(b".idata"), WRITABLE, built.bytes));
    }
    if let Some((bytes, directory)) = parts.exported {
        directories[0] = directory;
        chunks.push((label(b".edata"), READABLE, bytes));
    }
    if let Some((bytes, directory)) = parts.relocated {
        directories[5] = directory;
        chunks.push((label(b".reloc"), DISCARDABLE, bytes));
    }
    let preliminary = (0x80 + 24 + 240 + chunks.len() * 40) as u64;
    let limit = align(preliminary, u64::from(SECTOR)) as u32;
    let mut sections = Vec::with_capacity(chunks.len());
    let mut payloads = Vec::with_capacity(chunks.len());
    let mut address = VIRTUAL;
    let mut offset = limit;
    for (name, flags, mut raw) in chunks {
        let span = raw.len().max(1) as u32;
        if raw.is_empty() {
            raw.push(0);
        }
        pad(&mut raw, SECTOR as usize);
        let size = raw.len() as u32;
        sections.push(Section {
            label: name,
            address,
            span,
            offset,
            raw: size,
            flags,
        });
        address = advance(address, span as usize);
        offset += size;
        payloads.push(raw);
    }
    let first = sections[0].raw;
    let envelope = Envelope {
        shared: parts.shared,
        entry: parts.entry,
        base: parts.base,
        code: first,
        headers: limit,
        image: parts.image,
        directories,
        sections,
    };
    let mut out = headers(&envelope);
    for raw in payloads {
        out.extend_from_slice(&raw);
    }
    out
}
