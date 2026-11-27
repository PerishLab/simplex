use crate::dynamic::Survey;
use crate::format::{self, Buffer, Head, Item, Record, Segment, Stub};
use crate::image::Artifact;
use crate::layout::{Layout, Tables};
use crate::{INTERPRETER, Mode, PAGE, Plan};

pub struct Bundle {
    pub tables: Tables,
    pub layout: Layout,
    pub artifact: Artifact,
}

pub fn emit(plan: &Plan, survey: &Survey, bundle: &Bundle) -> Vec<u8> {
    let layout = &bundle.layout;
    let map = &layout.map;
    let entry = match plan.mode {
        Mode::Dynamic => map.code + plan.entry,
        _ => 0,
    };
    let mut out = Buffer::new();
    out.header(&Head {
        kind: format::SHARED,
        entry,
        count: layout.count,
    });
    segments(plan.mode, layout, &mut out);
    metadata(plan, survey, bundle, &mut out);
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

fn metadata(plan: &Plan, survey: &Survey, bundle: &Bundle, out: &mut Buffer) {
    let layout = &bundle.layout;
    let map = &layout.map;
    let tables = &bundle.tables;
    if plan.mode == Mode::Dynamic {
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
    for (item, &name) in plan.exports.iter().zip(&tables.exported) {
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
    for record in &bundle.artifact.records {
        out.record(record);
    }
    out.pad(layout.jumps);
    let shifted = tables.exported.len();
    for (index, name) in survey.procedures.iter().enumerate() {
        let import = survey.imported[name];
        let slot = layout.linkage + 8 * (3 + index as u64);
        out.record(&Record {
            offset: slot,
            symbol: (1 + shifted + import) as u32,
            code: format::SLOT,
            addend: 0,
        });
    }
}

fn payload(survey: &Survey, bundle: &Bundle, out: &mut Buffer) {
    let layout = &bundle.layout;
    let map = &layout.map;
    let artifact = &bundle.artifact;
    if layout.executable {
        out.pad(map.code);
        out.push(&artifact.code);
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
    out.push(&artifact.data);
    out.pad(map.globals);
    for &value in &artifact.globals {
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
