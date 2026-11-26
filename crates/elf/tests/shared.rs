use simplex_elf::{Kind, Mode, PAGE, Plan, Relocation, Symbol, write};

#[derive(Debug, PartialEq, Eq)]
struct Found {
    info: u8,
    section: u16,
    value: u64,
    size: u64,
}

struct Probe<'a> {
    bytes: &'a [u8],
}

impl<'a> Probe<'a> {
    fn half(&self, offset: usize) -> u16 {
        u16::from_le_bytes(self.bytes[offset..offset + 2].try_into().unwrap())
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }

    fn quad(&self, offset: usize) -> u64 {
        u64::from_le_bytes(self.bytes[offset..offset + 8].try_into().unwrap())
    }

    fn scan(&self, start: usize, size: usize, wanted: i64) -> Option<u64> {
        for cursor in (start..start + size).step_by(16) {
            if self.quad(cursor) as i64 == wanted {
                return Some(self.quad(cursor + 8));
            }
        }
        None
    }

    fn tag(&self, wanted: i64) -> Option<u64> {
        let count = usize::from(self.half(56));
        for index in 0..count {
            let pos = 64 + index * 56;
            if self.word(pos) == 2 {
                let start = self.quad(pos + 8) as usize;
                let size = self.quad(pos + 32) as usize;
                return self.scan(start, size, wanted);
            }
        }
        None
    }

    fn lookup(&self, query: &str) -> Option<Found> {
        let hash = self.tag(4)? as usize;
        let strings = self.tag(5)? as usize;
        let symbols = self.tag(6)? as usize;
        let buckets = self.word(hash) as usize;
        let slot = (digest(query.as_bytes()) as usize) % buckets;
        let mut current = self.word(hash + 8 + slot * 4) as usize;
        while current != 0 {
            let sym = symbols + current * 24;
            let off = self.word(sym) as usize;
            let start = strings + off;
            let end = self.bytes[start..].iter().position(|&b| b == 0).unwrap() + start;
            if &self.bytes[start..end] == query.as_bytes() {
                return Some(Found {
                    info: self.bytes[sym + 4],
                    section: self.half(sym + 6),
                    value: self.quad(sym + 8),
                    size: self.quad(sym + 16),
                });
            }
            current = self.word(hash + 8 + (buckets + current) * 4) as usize;
        }
        None
    }
}

fn digest(name: &[u8]) -> u32 {
    let mut state: u32 = 0;
    for &byte in name {
        state = (state << 4).wrapping_add(u32::from(byte));
        let top = state & 0xf000_0000;
        if top != 0 {
            state ^= top >> 24;
        }
        state &= !top;
    }
    state
}

#[test]
fn exports() {
    let plan = Plan {
        mode: Mode::Shared,
        entry: 0,
        needed: Vec::new(),
        code: vec![0xb8, 1, 0, 0, 0, 0xc3, 0xb8, 2, 0, 0, 0, 0xc3],
        data: vec![0u8; 16],
        exports: vec![
            Symbol {
                name: "sx$app$codec$encode".into(),
                offset: 0,
                size: 6,
                code: true,
            },
            Symbol {
                name: "sx$app$codec$decode".into(),
                offset: 6,
                size: 6,
                code: true,
            },
            Symbol {
                name: "sx$vt$app$codec$Codec$Stream".into(),
                offset: 0,
                size: 16,
                code: false,
            },
        ],
        imports: vec!["external".into()],
        relocations: vec![
            Relocation {
                offset: 0,
                symbol: "sx$app$codec$encode".into(),
                kind: Kind::Absolute64,
                addend: 0,
            },
            Relocation {
                offset: 8,
                symbol: "sx$app$codec$decode".into(),
                kind: Kind::Absolute64,
                addend: 0,
            },
        ],
    };

    let raw = write(&plan).expect("shared object");
    let probe = Probe { bytes: &raw };
    assert_eq!(probe.half(16), 3);
    assert_eq!(probe.quad(24), 0);
    assert_eq!(probe.tag(0x6fff_fffb), None);

    let count = usize::from(probe.half(56));
    for index in 0..count {
        assert_ne!(probe.word(64 + index * 56), 3);
    }

    let encode = probe.lookup("sx$app$codec$encode").expect("encode symbol");
    assert_eq!(
        encode,
        Found {
            info: 0x12,
            section: 1,
            value: PAGE,
            size: 6,
        }
    );

    let decode = probe.lookup("sx$app$codec$decode").expect("decode symbol");
    assert_eq!(
        decode,
        Found {
            info: 0x12,
            section: 1,
            value: PAGE + 6,
            size: 6,
        }
    );

    let vtable = probe
        .lookup("sx$vt$app$codec$Codec$Stream")
        .expect("vtable symbol");
    assert_eq!(
        vtable,
        Found {
            info: 0x11,
            section: 1,
            value: 2 * PAGE,
            size: 16,
        }
    );

    assert_eq!(probe.lookup("external"), None);
    assert_eq!(probe.lookup("missing"), None);

    let rela = probe.tag(7).expect("rela table") as usize;
    assert_eq!(probe.tag(8), Some(48));
    assert_eq!(probe.quad(rela), 2 * PAGE);
    assert_eq!(probe.quad(rela + 16), PAGE);
    assert_eq!(probe.quad(rela + 24), 2 * PAGE + 8);
    assert_eq!(probe.quad(rela + 40), PAGE + 6);
}

#[test]
fn codeless() {
    let plan = Plan {
        mode: Mode::Shared,
        entry: 0,
        needed: Vec::new(),
        code: Vec::new(),
        data: b"constant".to_vec(),
        exports: vec![Symbol {
            name: "sx$app$data$asset".into(),
            offset: 0,
            size: 8,
            code: false,
        }],
        imports: Vec::new(),
        relocations: Vec::new(),
    };
    let raw = write(&plan).expect("data-only shared library");
    let probe = Probe { bytes: &raw };
    let asset = probe.lookup("sx$app$data$asset").expect("asset symbol");
    assert_eq!(
        asset,
        Found {
            info: 0x11,
            section: 1,
            value: PAGE,
            size: 8,
        }
    );
}
