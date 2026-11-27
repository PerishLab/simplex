use super::format::Directory;
use super::survey::Group;
use crate::Import;

pub struct Imported {
    pub bytes: Vec<u8>,
    pub slots: Vec<u32>,
    pub table: Directory,
    pub addresses: Directory,
}

pub fn imports(base: u32, list: &[Import], groups: &[Group]) -> Imported {
    let count: usize = groups.iter().map(|g| g.members.len() + 1).sum();
    let span = (count * 8) as u32;
    let width = ((groups.len() + 1) * 20) as u32;
    let origin = base + span * 2 + width;
    let mut strings = Vec::new();
    let mut hints = vec![0u32; list.len()];
    let mut slots = vec![0u32; list.len()];
    for group in groups {
        for &member in &group.members {
            if !strings.len().is_multiple_of(2) {
                strings.push(0);
            }
            hints[member] = origin + strings.len() as u32;
            strings.extend_from_slice(&0u16.to_le_bytes());
            strings.extend_from_slice(list[member].name.as_bytes());
            strings.push(0);
        }
    }
    let mut names = Vec::with_capacity(groups.len());
    for group in groups {
        names.push(origin + strings.len() as u32);
        strings.extend_from_slice(group.library.as_bytes());
        strings.push(0);
    }
    let mut thunks = Vec::with_capacity(count * 8);
    let mut descriptors = Vec::with_capacity((groups.len() + 1) * 20);
    let mut cursor = 0u32;
    for (index, group) in groups.iter().enumerate() {
        let first = base + cursor;
        let original = base + span + cursor;
        for &member in &group.members {
            slots[member] = base + cursor;
            thunks.extend_from_slice(&u64::from(hints[member]).to_le_bytes());
            cursor += 8;
        }
        thunks.extend_from_slice(&0u64.to_le_bytes());
        cursor += 8;
        descriptors.extend_from_slice(&original.to_le_bytes());
        descriptors.extend_from_slice(&0u32.to_le_bytes());
        descriptors.extend_from_slice(&0u32.to_le_bytes());
        descriptors.extend_from_slice(&names[index].to_le_bytes());
        descriptors.extend_from_slice(&first.to_le_bytes());
    }
    descriptors.extend_from_slice(&[0u8; 20]);
    let mut bytes = Vec::with_capacity(thunks.len() * 2 + descriptors.len() + strings.len());
    bytes.extend_from_slice(&thunks);
    bytes.extend_from_slice(&thunks);
    bytes.extend_from_slice(&descriptors);
    bytes.extend_from_slice(&strings);
    Imported {
        bytes,
        slots,
        table: Directory {
            address: base + span * 2,
            size: width,
        },
        addresses: Directory {
            address: base,
            size: span,
        },
    }
}

pub fn exports(base: u32, module: &str, items: &[(String, u32)]) -> (Vec<u8>, Directory) {
    let count = items.len() as u32;
    let functions = if count == 0 { 0 } else { base + 40 };
    let names = if count == 0 { 0 } else { functions + count * 4 };
    let ordinals = if count == 0 { 0 } else { names + count * 4 };
    let start = 40 + (items.len() * 10);
    let mut strings = Vec::new();
    let banner = dll(module);
    let title = base + start as u32;
    strings.extend_from_slice(banner.as_bytes());
    strings.push(0);
    let mut pointers = Vec::with_capacity(items.len());
    for (symbol, _) in items {
        pointers.push(base + (start + strings.len()) as u32);
        strings.extend_from_slice(symbol.as_bytes());
        strings.push(0);
    }
    let mut bytes = Vec::with_capacity(start + strings.len());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&title.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&functions.to_le_bytes());
    bytes.extend_from_slice(&names.to_le_bytes());
    bytes.extend_from_slice(&ordinals.to_le_bytes());
    for &(_, target) in items {
        bytes.extend_from_slice(&target.to_le_bytes());
    }
    for &address in &pointers {
        bytes.extend_from_slice(&address.to_le_bytes());
    }
    for index in 0..items.len() {
        bytes.extend_from_slice(&(index as u16).to_le_bytes());
    }
    bytes.extend_from_slice(&strings);
    let directory = Directory {
        address: base,
        size: bytes.len() as u32,
    };
    (bytes, directory)
}

pub fn relocations(base: u32, entries: &[u32], shared: bool) -> Option<(Vec<u8>, Directory)> {
    if entries.is_empty() && !shared {
        return None;
    }
    let mut sorted = entries.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut bytes = Vec::new();
    if sorted.is_empty() {
        bytes.extend_from_slice(&0x1000u32.to_le_bytes());
        bytes.extend_from_slice(&12u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
    }
    let mut index = 0usize;
    while index < sorted.len() {
        let page = sorted[index] & !0xFFFu32;
        let mut finish = index + 1;
        while finish < sorted.len() && (sorted[finish] & !0xFFFu32) == page {
            finish += 1;
        }
        block(&mut bytes, page, &sorted[index..finish]);
        index = finish;
    }
    let size = bytes.len() as u32;
    Some((
        bytes,
        Directory {
            address: base,
            size,
        },
    ))
}

fn block(out: &mut Vec<u8>, page: u32, items: &[u32]) {
    let odd = !items.len().is_multiple_of(2);
    let total = 8 + (items.len() + usize::from(odd)) * 2;
    out.extend_from_slice(&page.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    for &address in items {
        let word = (10u16 << 12) | ((address & 0xFFF) as u16);
        out.extend_from_slice(&word.to_le_bytes());
    }
    if odd {
        out.extend_from_slice(&0u16.to_le_bytes());
    }
}

fn dll(module: &str) -> String {
    if module.is_empty() {
        return "module.dll".to_owned();
    }
    if module.ends_with(".dll") {
        return module.to_owned();
    }
    format!("{module}.dll")
}
