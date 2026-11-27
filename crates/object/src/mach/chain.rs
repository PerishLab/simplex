use super::format::PAGE;
use super::survey::Imported;
use crate::Fault;
use crate::digest::align;

#[derive(Clone, Copy)]
pub enum Payload {
    Rebase(u64),
    Bind { ordinal: u32, addend: i64 },
}

#[derive(Clone, Copy)]
pub struct Node {
    pub offset: u64,
    pub payload: Payload,
}

pub struct Topology {
    pub segments: usize,
    pub target: Option<usize>,
    pub offset: u64,
    pub size: u64,
}

pub fn apply(
    segment: &mut [u8],
    nodes: &[Node],
    topology: &Topology,
    imports: &[Imported],
) -> Result<Vec<u8>, Fault> {
    let mut sorted = nodes.to_vec();
    sorted.sort_by_key(|item| item.offset);
    for window in sorted.windows(2) {
        if window[0].offset == window[1].offset {
            return Err(Fault::Invalid("duplicate chained fixup offset".into()));
        }
    }
    let pages = (topology.size / PAGE) as usize;
    let mut starts = vec![0xFFFFu16; pages];
    let mut index = 0usize;
    while index < sorted.len() {
        let page = (sorted[index].offset / PAGE) as usize;
        starts[page] = (sorted[index].offset % PAGE) as u16;
        let mut finish = index + 1;
        while finish < sorted.len() && (sorted[finish].offset / PAGE) as usize == page {
            finish += 1;
        }
        chain(segment, &sorted[index..finish]);
        index = finish;
    }
    Ok(serialize(topology, &starts, &sorted, imports))
}

fn chain(segment: &mut [u8], group: &[Node]) {
    for (position, item) in group.iter().enumerate() {
        let next = match group.get(position + 1) {
            Some(following) => ((following.offset - item.offset) / 4) & 0xFFF,
            None => 0,
        };
        let encoded = match item.payload {
            Payload::Rebase(target) => (next << 51) | (target & 0x000F_FFFF_FFFF),
            Payload::Bind { ordinal, addend } => {
                let low = u64::from(ordinal) & 0x00FF_FFFF;
                let signed = u64::from((addend as i8) as u8) << 24;
                (1u64 << 63) | (next << 51) | signed | low
            }
        };
        let start = item.offset as usize;
        segment[start..start + 8].copy_from_slice(&encoded.to_le_bytes());
    }
}

fn serialize(topology: &Topology, starts: &[u16], nodes: &[Node], imports: &[Imported]) -> Vec<u8> {
    let active = !nodes.is_empty() && topology.target.is_some();
    let words = 1 + topology.segments;
    let prefix = align((words * 4) as u64, 8) as usize;
    let mut map = Vec::new();
    map.extend_from_slice(&(topology.segments as u32).to_le_bytes());
    for slot in 0..topology.segments {
        let pointer = match topology.target {
            Some(wanted) if active && slot == wanted => prefix as u32,
            _ => 0,
        };
        map.extend_from_slice(&pointer.to_le_bytes());
    }
    map.resize(prefix, 0);
    if active {
        let count = starts.len();
        let width = align((22 + count * 2) as u64, 8) as usize;
        let mark = map.len();
        map.extend_from_slice(&(width as u32).to_le_bytes());
        map.extend_from_slice(&(PAGE as u16).to_le_bytes());
        map.extend_from_slice(&6u16.to_le_bytes());
        map.extend_from_slice(&topology.offset.to_le_bytes());
        map.extend_from_slice(&0u32.to_le_bytes());
        map.extend_from_slice(&(count as u16).to_le_bytes());
        for &offset in starts {
            map.extend_from_slice(&offset.to_le_bytes());
        }
        map.resize(mark + width, 0);
    }

    let mut symbols = vec![0u8];
    let mut records = Vec::with_capacity(imports.len() * 4);
    for item in imports {
        let position = symbols.len() as u32;
        symbols.extend_from_slice(item.name.as_bytes());
        symbols.push(0);
        let word = u32::from(item.ordinal) | (position << 9);
        records.extend_from_slice(&word.to_le_bytes());
    }

    let first = 32u32;
    let second = align(u64::from(first) + map.len() as u64, 8) as u32;
    let third = second + records.len() as u32;
    let total = align(u64::from(third) + symbols.len() as u64, 8) as usize;

    let mut out = vec![0u8; total];
    out[4..8].copy_from_slice(&first.to_le_bytes());
    out[8..12].copy_from_slice(&second.to_le_bytes());
    out[12..16].copy_from_slice(&third.to_le_bytes());
    out[16..20].copy_from_slice(&(imports.len() as u32).to_le_bytes());
    out[20..24].copy_from_slice(&1u32.to_le_bytes());
    let alpha = first as usize;
    out[alpha..alpha + map.len()].copy_from_slice(&map);
    let bravo = second as usize;
    out[bravo..bravo + records.len()].copy_from_slice(&records);
    let charlie = third as usize;
    out[charlie..charlie + symbols.len()].copy_from_slice(&symbols);
    out
}
