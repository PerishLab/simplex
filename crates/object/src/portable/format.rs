pub const VIRTUAL: u32 = 0x1000;
pub const SECTOR: u32 = 0x0200;
pub const EXECUTABLE: u64 = 0x0001_4000_0000;
pub const SHARED: u64 = 0x0001_8000_0000;

pub const CODE: u32 = 0x6000_0020;
pub const WRITABLE: u32 = 0xC000_0040;
pub const READABLE: u32 = 0x4000_0040;
pub const DISCARDABLE: u32 = 0x4200_0040;

#[derive(Clone, Copy, Default)]
pub struct Directory {
    pub address: u32,
    pub size: u32,
}

pub struct Section {
    pub label: [u8; 8],
    pub address: u32,
    pub span: u32,
    pub offset: u32,
    pub raw: u32,
    pub flags: u32,
}

pub struct Envelope {
    pub shared: bool,
    pub entry: u32,
    pub base: u64,
    pub code: u32,
    pub headers: u32,
    pub image: u32,
    pub directories: [Directory; 16],
    pub sections: Vec<Section>,
}

pub fn label(text: &[u8]) -> [u8; 8] {
    let mut out = [0u8; 8];
    let count = text.len().min(8);
    out[..count].copy_from_slice(&text[..count]);
    out
}

pub fn pad(buffer: &mut Vec<u8>, step: usize) {
    let remainder = buffer.len() % step;
    if remainder != 0 {
        buffer.resize(buffer.len() + (step - remainder), 0);
    }
}

pub fn patch(bytes: &mut [u8], offset: u64, value: i32) {
    let start = offset as usize;
    if let Some(slice) = bytes.get_mut(start..start + 4) {
        slice.copy_from_slice(&value.to_le_bytes());
    }
}

pub fn store(bytes: &mut [u8], offset: u64, value: u64) {
    let start = offset as usize;
    if let Some(slice) = bytes.get_mut(start..start + 8) {
        slice.copy_from_slice(&value.to_le_bytes());
    }
}

pub fn headers(envelope: &Envelope) -> Vec<u8> {
    let mut out = vec![0u8; 0x80];
    out[0..2].copy_from_slice(b"MZ");
    out[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    coff(&mut out, envelope.sections.len() as u16, envelope.shared);
    optional(&mut out, envelope);
    for item in &envelope.sections {
        section(&mut out, item);
    }
    out.resize(envelope.headers as usize, 0);
    out
}

fn coff(out: &mut Vec<u8>, count: u16, shared: bool) {
    let flags: u16 = match shared {
        true => 0x2022,
        false => 0x0022,
    };
    out.extend_from_slice(b"PE\0\0");
    out.extend_from_slice(&0x8664u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(&240u16.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
}

fn optional(out: &mut Vec<u8>, envelope: &Envelope) {
    let mut fixed = [0u8; 112];
    fixed[0..2].copy_from_slice(&0x020Bu16.to_le_bytes());
    fixed[2] = 14;
    fixed[4..8].copy_from_slice(&envelope.code.to_le_bytes());
    fixed[16..20].copy_from_slice(&envelope.entry.to_le_bytes());
    fixed[20..24].copy_from_slice(&VIRTUAL.to_le_bytes());
    fixed[24..32].copy_from_slice(&envelope.base.to_le_bytes());
    fixed[32..36].copy_from_slice(&VIRTUAL.to_le_bytes());
    fixed[36..40].copy_from_slice(&SECTOR.to_le_bytes());
    fixed[40..42].copy_from_slice(&6u16.to_le_bytes());
    fixed[48..50].copy_from_slice(&6u16.to_le_bytes());
    fixed[56..60].copy_from_slice(&envelope.image.to_le_bytes());
    fixed[60..64].copy_from_slice(&envelope.headers.to_le_bytes());
    fixed[68..70].copy_from_slice(&3u16.to_le_bytes());
    fixed[70..72].copy_from_slice(&0x0160u16.to_le_bytes());
    fixed[72..80].copy_from_slice(&0x0010_0000u64.to_le_bytes());
    fixed[80..88].copy_from_slice(&0x0000_1000u64.to_le_bytes());
    fixed[88..96].copy_from_slice(&0x0010_0000u64.to_le_bytes());
    fixed[96..104].copy_from_slice(&0x0000_1000u64.to_le_bytes());
    fixed[108..112].copy_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&fixed);
    for item in &envelope.directories {
        out.extend_from_slice(&item.address.to_le_bytes());
        out.extend_from_slice(&item.size.to_le_bytes());
    }
}

fn section(out: &mut Vec<u8>, item: &Section) {
    let mut raw = [0u8; 40];
    raw[0..8].copy_from_slice(&item.label);
    raw[8..12].copy_from_slice(&item.span.to_le_bytes());
    raw[12..16].copy_from_slice(&item.address.to_le_bytes());
    raw[16..20].copy_from_slice(&item.raw.to_le_bytes());
    raw[20..24].copy_from_slice(&item.offset.to_le_bytes());
    raw[36..40].copy_from_slice(&item.flags.to_le_bytes());
    out.extend_from_slice(&raw);
}
