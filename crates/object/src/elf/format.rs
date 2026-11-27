pub const LOAD: u32 = 1;
pub const DYNAMIC: u32 = 2;
pub const INTERP: u32 = 3;
pub const HEADER: u32 = 6;
pub const STACK: u32 = 0x6474_e551;

pub const EXECUTE: u32 = 1;
pub const WRITE: u32 = 2;
pub const READ: u32 = 4;

pub const EXECUTABLE: u16 = 2;
pub const SHARED: u16 = 3;
pub const MACHINE: u16 = 62;

pub const DIRECT: u32 = 1;
pub const GLOBAL: u32 = 6;
pub const SLOT: u32 = 7;
pub const RELATIVE: u32 = 8;

pub const NEEDED: i64 = 1;
pub const TOTAL: i64 = 2;
pub const TABLE: i64 = 3;
pub const HASH: i64 = 4;
pub const STRINGS: i64 = 5;
pub const SYMBOLS: i64 = 6;
pub const RECORDS: i64 = 7;
pub const SPAN: i64 = 8;
pub const STRIDE: i64 = 9;
pub const LENGTH: i64 = 10;
pub const UNIT: i64 = 11;
pub const FORMAT: i64 = 20;
pub const JUMPS: i64 = 23;
pub const SEARCH: i64 = 29;
pub const STATE: i64 = 0x6fff_fffb;
pub const POSITION: u64 = 0x0800_0000;

pub struct Head {
    pub kind: u16,
    pub entry: u64,
    pub count: u16,
}

pub struct Segment {
    pub kind: u32,
    pub flags: u32,
    pub offset: u64,
    pub address: u64,
    pub size: u64,
    pub align: u64,
}

pub struct Item {
    pub name: u32,
    pub info: u8,
    pub section: u16,
    pub value: u64,
    pub size: u64,
}

pub struct Entry {
    pub target: u64,
    pub descriptor: u64,
    pub bias: i64,
}

impl Entry {
    pub fn pack(target: u64, symbol: u32, kind: u32, bias: i64) -> Self {
        Self {
            target,
            descriptor: (u64::from(symbol) << 32) | u64::from(kind),
            bias,
        }
    }
}

pub struct Stub {
    pub index: u32,
    pub root: u64,
    pub origin: u64,
    pub slot: u64,
}

pub struct Buffer {
    bytes: Vec<u8>,
}

impl Buffer {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    pub fn push(&mut self, slice: &[u8]) {
        self.bytes.extend_from_slice(slice);
    }

    pub fn quad(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn pad(&mut self, target: u64) {
        let limit = target as usize;
        if self.bytes.len() < limit {
            self.bytes.resize(limit, 0);
        }
    }

    pub fn header(&mut self, head: &Head) {
        let mut raw = [0u8; 64];
        raw[0..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
        raw[16..18].copy_from_slice(&head.kind.to_le_bytes());
        raw[18..20].copy_from_slice(&MACHINE.to_le_bytes());
        raw[20..24].copy_from_slice(&1u32.to_le_bytes());
        raw[24..32].copy_from_slice(&head.entry.to_le_bytes());
        raw[32..40].copy_from_slice(&64u64.to_le_bytes());
        raw[52..54].copy_from_slice(&64u16.to_le_bytes());
        raw[54..56].copy_from_slice(&56u16.to_le_bytes());
        raw[56..58].copy_from_slice(&head.count.to_le_bytes());
        self.push(&raw);
    }

    pub fn segment(&mut self, item: &Segment) {
        let mut raw = [0u8; 56];
        raw[0..4].copy_from_slice(&item.kind.to_le_bytes());
        raw[4..8].copy_from_slice(&item.flags.to_le_bytes());
        raw[8..16].copy_from_slice(&item.offset.to_le_bytes());
        raw[16..24].copy_from_slice(&item.address.to_le_bytes());
        raw[24..32].copy_from_slice(&item.address.to_le_bytes());
        raw[32..40].copy_from_slice(&item.size.to_le_bytes());
        raw[40..48].copy_from_slice(&item.size.to_le_bytes());
        raw[48..56].copy_from_slice(&item.align.to_le_bytes());
        self.push(&raw);
    }

    pub fn symbol(&mut self, item: &Item) {
        let mut raw = [0u8; 24];
        raw[0..4].copy_from_slice(&item.name.to_le_bytes());
        raw[4] = item.info;
        raw[6..8].copy_from_slice(&item.section.to_le_bytes());
        raw[8..16].copy_from_slice(&item.value.to_le_bytes());
        raw[16..24].copy_from_slice(&item.size.to_le_bytes());
        self.push(&raw);
    }

    pub fn record(&mut self, item: &Entry) {
        let mut raw = [0u8; 24];
        raw[0..8].copy_from_slice(&item.target.to_le_bytes());
        raw[8..16].copy_from_slice(&item.descriptor.to_le_bytes());
        raw[16..24].copy_from_slice(&item.bias.to_le_bytes());
        self.push(&raw);
    }

    pub fn pair(&mut self, tag: i64, value: u64) {
        let mut raw = [0u8; 16];
        raw[0..8].copy_from_slice(&tag.to_le_bytes());
        raw[8..16].copy_from_slice(&value.to_le_bytes());
        self.push(&raw);
    }

    pub fn prologue(&mut self, origin: u64, table: u64) {
        let first = ((table + 8) as i64 - (origin + 6) as i64) as i32;
        let second = ((table + 16) as i64 - (origin + 12) as i64) as i32;
        let mut raw = [0u8; 16];
        raw[0..2].copy_from_slice(&[0xff, 0x35]);
        raw[2..6].copy_from_slice(&first.to_le_bytes());
        raw[6..8].copy_from_slice(&[0xff, 0x25]);
        raw[8..12].copy_from_slice(&second.to_le_bytes());
        raw[12..16].copy_from_slice(&[0x0f, 0x1f, 0x40, 0x00]);
        self.push(&raw);
    }

    pub fn stub(&mut self, item: &Stub) {
        let target = (item.slot as i64 - (item.origin + 6) as i64) as i32;
        let back = (item.root as i64 - (item.origin + 16) as i64) as i32;
        let mut raw = [0u8; 16];
        raw[0..2].copy_from_slice(&[0xff, 0x25]);
        raw[2..6].copy_from_slice(&target.to_le_bytes());
        raw[6] = 0x68;
        raw[7..11].copy_from_slice(&item.index.to_le_bytes());
        raw[11] = 0xe9;
        raw[12..16].copy_from_slice(&back.to_le_bytes());
        self.push(&raw);
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
