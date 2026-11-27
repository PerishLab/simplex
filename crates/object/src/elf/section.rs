use super::dynamic::Survey;
use super::format::{self, Buffer, Entry, Head, Item, Segment, Stub};
use super::image::Resolved;
use super::layout::{Layout, Tables};
use super::{INTERPRETER, PAGE};
use crate::{Artifact, Mode};

pub struct Bundle {
    pub tables: Tables,
    pub layout: Layout,
    pub resolved: Resolved,
}

pub fn emit(artifact: &Artifact, survey: &Survey, bundle: &Bundle) -> Vec<u8> {
    let layout = &bundle.layout;
    let map = &layout.map;
    let entry = match artifact.mode {
        Mode::Dynamic => map.code + artifact.entry,
        _ => 0,
    };
    let mut out = Buffer::new();
    out.header(&Head {
        kind: format::SHARED,
        entry,
        count: layout.count,
    });
    segments(artifact.mode, layout, &mut out);
    metadata(artifact, survey, bundle, &mut out);
    payload(survey, bundle, &mut out);
    out.finish()
}

fn segments(mode: Mode, layout: &Layout, out: &mut Buffer) {
    let map = &layout.map;
    out.segment(&Segment {
        kind: format::HEADER,
        flags: format::READ,
        offset: 64,
        address: 64,
        size: 56 * u64::from(layout.count),
        align: 8,
    });
    if mode == Mode::Dynamic {
        out.segment(&Segment {
            kind: format::INTERP,
            flags: format::READ,
            offset: layout.interp,
            address: layout.interp,
            size: INTERPRETER.len() as u64 + 1,
            align: 1,
        });
    }
    out.segment(&Segment {
        kind: format::LOAD,
        flags: format::READ,
        offset: 0,
        address: 0,
        size: layout.front,
        align: PAGE,
    });
    if layout.executable {
        out.segment(&Segment {
            kind: format::LOAD,
            flags: format::READ | format::EXECUTE,
            offset: map.code,
            address: map.code,
            size: layout.span,
            align: PAGE,
        });
    }
    out.segment(&Segment {
        kind: format::LOAD,
        flags: format::READ | format::WRITE,
        offset: map.data,
        address: map.data,
        size: layout.volume,
        align: PAGE,
    });
    out.segment(&Segment {
        kind: format::DYNAMIC,
        flags: format::READ | format::WRITE,
        offset: layout.dynamic,
        address: layout.dynamic,
        size: layout.tags,
        align: 8,
    });
    out.segment(&Segment {
        kind: format::STACK,
        flags: format::READ | format::WRITE,
        offset: 0,
        address: 0,
        size: 0,
        align: 16,
    });
}

fn metadata(artifact: &Artifact, survey: &Survey, bundle: &Bundle, out: &mut Buffer) {
    let layout = &bundle.layout;
    let map = &layout.map;
    let tables = &bundle.tables;
    if artifact.mode == Mode::Dynamic {
        out.pad(layout.interp);
        out.push(INTERPRETER.as_bytes());
        out.push(&[0]);
    }
    out.pad(layout.hash);
    out.push(&tables.hash);
    out.pad(layout.symbols);
    out.symbol(&Item {
        name: 0,
        info: 0,
        section: 0,
        value: 0,
        size: 0,
    });
    for (&index, &name) in survey.published.iter().zip(&tables.exported) {
        let item = &artifact.symbols[index];
        let info = if item.code { 0x12 } else { 0x11 };
        let value = map.symbol(item);
        out.symbol(&Item {
            name,
            info,
            section: 1,
            value,
            size: item.size,
        });
    }
    for &name in &tables.imported {
        out.symbol(&Item {
            name,
            info: 0x10,
            section: 0,
            value: 0,
            size: 0,
        });
    }
    out.pad(layout.strings);
    out.push(tables.pool.bytes());
    out.pad(layout.records);
    for record in &bundle.resolved.records {
        out.record(record);
    }
    out.pad(layout.jumps);
    let shifted = tables.exported.len();
    for (index, name) in survey.procedures.iter().enumerate() {
        let import = survey.imported[name];
        let slot = layout.linkage + 8 * (3 + index as u64);
        let symbol = (1 + shifted + import) as u32;
        out.record(&Entry::pack(slot, symbol, format::SLOT, 0));
    }
}

fn payload(survey: &Survey, bundle: &Bundle, out: &mut Buffer) {
    let layout = &bundle.layout;
    let map = &layout.map;
    let resolved = &bundle.resolved;
    if layout.executable {
        out.pad(map.code);
        out.push(&resolved.code);
        if !survey.procedures.is_empty() {
            out.pad(map.stubs);
            out.prologue(map.stubs, layout.linkage);
            for index in 0..survey.procedures.len() {
                let origin = map.stubs + 16 * (1 + index as u64);
                let slot = layout.linkage + 8 * (3 + index as u64);
                out.stub(&Stub {
                    index: index as u32,
                    root: map.stubs,
                    origin,
                    slot,
                });
            }
        }
    }
    out.pad(map.data);
    out.push(&resolved.data);
    out.pad(map.globals);
    for &value in &resolved.globals {
        out.quad(value);
    }
    if !survey.procedures.is_empty() {
        out.pad(layout.linkage);
        out.quad(layout.dynamic);
        out.quad(0);
        out.quad(0);
        for index in 0..survey.procedures.len() {
            let origin = map.stubs + 16 * (1 + index as u64);
            out.quad(origin + 6);
        }
    }
    out.pad(layout.dynamic);
    for &(tag, value) in &layout.pairs {
        out.pair(tag, value);
    }
}
