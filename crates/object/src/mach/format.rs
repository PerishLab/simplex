use crate::Fault;
use crate::digest::align;

pub const PAGE: u64 = 0x4000;
pub const BASE: u64 = 0x0001_0000_0000;
pub const SYSTEM: &str = "/usr/lib/libSystem.B.dylib";

pub fn normalize(library: &str) -> String {
    if library.is_empty() || library == "libSystem.B.dylib" || library == SYSTEM {
        return SYSTEM.to_owned();
    }
    if library.starts_with('/') || library.starts_with('@') {
        return library.to_owned();
    }
    if library.ends_with(".dylib") {
        return format!("@executable_path/{library}");
    }
    format!("@executable_path/{library}.dylib")
}

pub fn identity(name: &str) -> String {
    if name.is_empty() {
        return "@executable_path/module.dylib".to_owned();
    }
    if name.starts_with('@') || name.starts_with('/') {
        return name.to_owned();
    }
    if name.ends_with(".dylib") {
        return format!("@executable_path/{name}");
    }
    format!("@executable_path/{name}.dylib")
}

pub struct Segment {
    pub name: &'static [u8],
    pub address: u64,
    pub span: u64,
    pub offset: u64,
    pub size: u64,
    pub protection: i32,
    pub flags: u32,
    pub sections: Vec<Section>,
}

pub struct Section {
    pub name: &'static [u8],
    pub segment: &'static [u8],
    pub address: u64,
    pub size: u64,
    pub offset: u32,
    pub align: u32,
    pub flags: u32,
    pub stride: u32,
}

fn fixed(text: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    let count = text.len().min(16);
    out[..count].copy_from_slice(&text[..count]);
    out
}

pub fn header(shared: bool, commands: u32, size: u32) -> [u8; 32] {
    let kind: u32 = if shared { 6 } else { 2 };
    let flags: u32 = if shared { 0x0010_0085 } else { 0x0020_0085 };
    let mut raw = [0u8; 32];
    raw[0..4].copy_from_slice(&0xFEED_FACFu32.to_le_bytes());
    raw[4..8].copy_from_slice(&0x0100_000Cu32.to_le_bytes());
    raw[8..12].copy_from_slice(&0u32.to_le_bytes());
    raw[12..16].copy_from_slice(&kind.to_le_bytes());
    raw[16..20].copy_from_slice(&commands.to_le_bytes());
    raw[20..24].copy_from_slice(&size.to_le_bytes());
    raw[24..28].copy_from_slice(&flags.to_le_bytes());
    raw
}

pub fn segment(item: &Segment) -> Vec<u8> {
    let total = 72 + item.sections.len() * 80;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x19u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&fixed(item.name));
    out.extend_from_slice(&item.address.to_le_bytes());
    out.extend_from_slice(&item.span.to_le_bytes());
    out.extend_from_slice(&item.offset.to_le_bytes());
    out.extend_from_slice(&item.size.to_le_bytes());
    out.extend_from_slice(&item.protection.to_le_bytes());
    out.extend_from_slice(&item.protection.to_le_bytes());
    out.extend_from_slice(&(item.sections.len() as u32).to_le_bytes());
    out.extend_from_slice(&item.flags.to_le_bytes());
    for entry in &item.sections {
        section(&mut out, entry);
    }
    out
}

fn section(out: &mut Vec<u8>, item: &Section) {
    out.extend_from_slice(&fixed(item.name));
    out.extend_from_slice(&fixed(item.segment));
    out.extend_from_slice(&item.address.to_le_bytes());
    out.extend_from_slice(&item.size.to_le_bytes());
    out.extend_from_slice(&item.offset.to_le_bytes());
    out.extend_from_slice(&item.align.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&item.flags.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&item.stride.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
}

pub fn dylib(command: u32, path: &str) -> Vec<u8> {
    let total = align((24 + path.len() + 1) as u64, 8) as usize;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&command.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&24u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&0x0001_0000u32.to_le_bytes());
    out.extend_from_slice(&0x0001_0000u32.to_le_bytes());
    out.extend_from_slice(path.as_bytes());
    out.resize(total, 0);
    out
}

pub fn linker(path: &str) -> Vec<u8> {
    let total = align((12 + path.len() + 1) as u64, 8) as usize;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x0Eu32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&12u32.to_le_bytes());
    out.extend_from_slice(path.as_bytes());
    out.resize(total, 0);
    out
}

pub fn uuid(bytes: &[u8; 16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&0x1Bu32.to_le_bytes());
    out.extend_from_slice(&24u32.to_le_bytes());
    out.extend_from_slice(bytes);
    out
}

pub fn version() -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&0x32u32.to_le_bytes());
    out.extend_from_slice(&24u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0x000E_0000u32.to_le_bytes());
    out.extend_from_slice(&0x000E_0000u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

pub fn main(offset: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&0x8000_0028u32.to_le_bytes());
    out.extend_from_slice(&24u32.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out
}

pub fn linkedit(command: u32, offset: u32, size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&command.to_le_bytes());
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub fn symtab(symbols: (u32, u32), strings: (u32, u32)) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&0x02u32.to_le_bytes());
    out.extend_from_slice(&24u32.to_le_bytes());
    out.extend_from_slice(&symbols.0.to_le_bytes());
    out.extend_from_slice(&symbols.1.to_le_bytes());
    out.extend_from_slice(&strings.0.to_le_bytes());
    out.extend_from_slice(&strings.1.to_le_bytes());
    out
}

pub fn dysymtab(locals: u32, externals: u32) -> Vec<u8> {
    let mut words = [0u32; 20];
    words[0] = 0x0B;
    words[1] = 80;
    words[2] = 0;
    words[3] = locals;
    words[4] = locals;
    words[5] = externals;
    words[6] = locals + externals;
    words[7] = 0;
    let mut out = Vec::with_capacity(80);
    for word in words {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out
}

pub struct Coder<'a> {
    pub bytes: &'a mut [u8],
}

impl<'a> Coder<'a> {
    fn read(&self, offset: u64) -> u32 {
        let start = offset as usize;
        u32::from_le_bytes([
            self.bytes[start],
            self.bytes[start + 1],
            self.bytes[start + 2],
            self.bytes[start + 3],
        ])
    }

    fn write(&mut self, offset: u64, word: u32) {
        let start = offset as usize;
        self.bytes[start..start + 4].copy_from_slice(&word.to_le_bytes());
    }

    pub fn paged(&self, offset: u64) -> bool {
        (self.read(offset) & 0x9F00_0000) == 0x9000_0000
    }

    pub fn branch(&mut self, offset: u64, place: u64, target: i64) -> Result<(), Fault> {
        let delta = target - (place as i64);
        if delta % 4 != 0 || !(-0x0800_0000..0x0800_0000).contains(&delta) {
            return Err(Fault::Invalid("branch displacement out of range".into()));
        }
        let current = self.read(offset);
        let base = if current == 0 {
            0x9400_0000
        } else {
            current & 0xFC00_0000
        };
        let immediate = ((delta >> 2) as u32) & 0x03FF_FFFF;
        self.write(offset, base | immediate);
        Ok(())
    }

    pub fn page(&mut self, offset: u64, place: u64, target: i64) -> Result<(), Fault> {
        let origin = (place as i64) & !0xFFF;
        let destination = target & !0xFFF;
        let pages = (destination - origin) >> 12;
        if !(-0x0010_0000..0x0010_0000).contains(&pages) {
            return Err(Fault::Invalid("page displacement out of range".into()));
        }
        let current = self.read(offset);
        let low = ((pages as u32) & 0x3) << 29;
        let high = (((pages as u32) >> 2) & 0x7FFFF) << 5;
        self.write(offset, (current & 0x9F00_001F) | low | high);
        Ok(())
    }

    pub fn low(&mut self, offset: u64, scale: u8, target: i64) -> Result<(), Fault> {
        if scale > 4 {
            return Err(Fault::Invalid("invalid low scale".into()));
        }
        let remainder = (target as u64) & 0xFFF;
        let mask = (1u64 << scale) - 1;
        if (remainder & mask) != 0 {
            return Err(Fault::Invalid("unaligned scaled page offset".into()));
        }
        let current = self.read(offset);
        let immediate = ((remainder >> scale) as u32) & 0xFFF;
        self.write(offset, (current & 0xFFC0_03FF) | (immediate << 10));
        Ok(())
    }

    pub fn relative(&mut self, offset: u64, place: u64, target: i64) -> Result<(), Fault> {
        let current = self.read(offset);
        if (current & 0xFC00_0000) == 0x1400_0000 {
            return self.branch(offset, place, target);
        }
        let delta = target - (place as i64);
        if !(-0x0010_0000..0x0010_0000).contains(&delta) {
            return Err(Fault::Invalid("relative displacement out of range".into()));
        }
        let low = ((delta as u32) & 0x3) << 29;
        let high = (((delta >> 2) as u32) & 0x7FFFF) << 5;
        self.write(offset, (current & 0x9F00_001F) | low | high);
        Ok(())
    }
}

pub fn stub(place: u64, slot: u64) -> Result<[u8; 12], Fault> {
    let mut raw = [0u8; 12];
    raw[0..4].copy_from_slice(&0x9000_0010u32.to_le_bytes());
    raw[4..8].copy_from_slice(&0xF940_0210u32.to_le_bytes());
    raw[8..12].copy_from_slice(&0xD61F_0200u32.to_le_bytes());
    let mut coder = Coder { bytes: &mut raw };
    coder.page(0, place, slot as i64)?;
    coder.low(4, 3, slot as i64)?;
    Ok(raw)
}
