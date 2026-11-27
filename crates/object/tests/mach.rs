use simplex_object::{Artifact, Fault, Fixup, Format, Import, Mode, Relocation, Symbol, emit};

struct Scanner<'a> {
    bytes: &'a [u8],
}

impl<'a> Scanner<'a> {
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }

    fn big(&self, offset: usize) -> u32 {
        u32::from_be_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }

    fn quad(&self, offset: usize) -> u64 {
        u64::from_le_bytes(self.bytes[offset..offset + 8].try_into().unwrap())
    }

    fn commands(&self) -> Vec<(u32, usize, usize)> {
        let count = self.word(16) as usize;
        let mut cursor = 32usize;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let command = self.word(cursor);
            let size = self.word(cursor + 4) as usize;
            out.push((command, cursor, size));
            cursor += size;
        }
        out
    }

    fn find(&self, wanted: u32) -> Option<(usize, usize)> {
        self.commands()
            .into_iter()
            .find(|&(command, _, _)| command == wanted)
            .map(|(_, offset, size)| (offset, size))
    }
}

fn decode(slice: &[u8], cursor: &mut usize) -> u64 {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = slice[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if (byte & 0x80) == 0 {
            return value;
        }
        shift += 7;
    }
}

fn lookup(trie: &[u8], query: &str) -> Option<u64> {
    let mut node = 0usize;
    let mut remaining = query.as_bytes();
    loop {
        let mut cursor = node;
        let terminal = decode(trie, &mut cursor) as usize;
        if remaining.is_empty() {
            if terminal == 0 {
                return None;
            }
            let _ = decode(trie, &mut cursor);
            return Some(decode(trie, &mut cursor));
        }
        cursor += terminal;
        let children = usize::from(trie[cursor]);
        cursor += 1;
        let mut matched = None;
        for _ in 0..children {
            let end = trie[cursor..].iter().position(|&b| b == 0).unwrap() + cursor;
            let edge = &trie[cursor..end];
            cursor = end + 1;
            let child = decode(trie, &mut cursor) as usize;
            if remaining.starts_with(edge) {
                matched = Some((edge.len(), child));
                break;
            }
        }
        let (consumed, next) = matched?;
        remaining = &remaining[consumed..];
        node = next;
    }
}

fn blank(mode: Mode) -> Artifact {
    Artifact {
        format: Format::Mach,
        mode,
        name: "codec".into(),
        entry: 0,
        code: Vec::new(),
        data: Vec::new(),
        symbols: Vec::new(),
        imports: Vec::new(),
        relocations: Vec::new(),
    }
}

#[test]
fn errors() {
    assert_eq!(emit(&blank(Mode::Standalone)), Err(Fault::Empty));

    let mut bad = blank(Mode::Dynamic);
    bad.code = vec![0, 0, 0, 0x94, 0xc0, 0x03, 0x5f, 0xd6];
    bad.relocations.push(Relocation {
        offset: 2,
        symbol: "_write".into(),
        kind: Fixup::Call,
        addend: 0,
    });
    assert!(matches!(emit(&bad), Err(Fault::Invalid(_))));

    bad.relocations[0].offset = 0;
    assert_eq!(emit(&bad), Err(Fault::Unresolved("_write".into())));
}

#[test]
fn executable() {
    let mut item = blank(Mode::Dynamic);
    item.name = "runner".into();
    item.code = vec![
        0x00, 0x00, 0x00, 0x94, 0x00, 0x00, 0x00, 0x90, 0x00, 0x00, 0x40, 0xf9, 0xc0, 0x03, 0x5f,
        0xd6, 0x20, 0x05, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6,
    ];
    item.data = vec![0u8; 16];
    item.symbols = vec![
        Symbol {
            name: "_main".into(),
            offset: 0,
            size: 16,
            code: true,
            exported: true,
        },
        Symbol {
            name: "_helper".into(),
            offset: 16,
            size: 8,
            code: true,
            exported: false,
        },
        Symbol {
            name: "_vtable".into(),
            offset: 0,
            size: 8,
            code: false,
            exported: false,
        },
    ];
    item.imports = vec![
        Import {
            name: "_write".into(),
            library: "/usr/lib/libSystem.B.dylib".into(),
            code: true,
        },
        Import {
            name: "_sx$external".into(),
            library: "codec".into(),
            code: false,
        },
    ];
    item.relocations = vec![
        Relocation {
            offset: 0,
            symbol: "_write".into(),
            kind: Fixup::Call,
            addend: 0,
        },
        Relocation {
            offset: 4,
            symbol: "_sx$external".into(),
            kind: Fixup::Page,
            addend: 0,
        },
        Relocation {
            offset: 8,
            symbol: "_sx$external".into(),
            kind: Fixup::Low { scale: 3 },
            addend: 0,
        },
        Relocation {
            offset: 0,
            symbol: "_helper".into(),
            kind: Fixup::Pointer,
            addend: 0,
        },
    ];

    let raw = emit(&item).expect("mach executable");
    let scan = Scanner { bytes: &raw };
    assert_eq!(scan.word(0), 0xFEED_FACF);
    assert_eq!(scan.word(4), 0x0100_000C);
    assert_eq!(scan.word(12), 2);

    for (command, position, _) in scan.commands() {
        if command == 0x19 {
            assert_eq!(scan.quad(position + 24) % 0x4000, 0);
            assert_eq!(scan.quad(position + 32) % 0x4000, 0);
        }
    }

    assert!(scan.find(0x1B).is_some());
    assert!(scan.find(0x32).is_some());
    assert!(scan.find(0x8000_0028).is_some());
    assert!(scan.find(0x8000_0034).is_some());
    assert!(scan.find(0x8000_0033).is_some());

    let (seal, _) = scan.find(0x1D).expect("code signature command");
    let start = scan.word(seal + 8) as usize;
    let length = scan.word(seal + 12) as usize;
    assert_eq!(start + length, raw.len());
    assert_eq!(scan.big(start), 0xFADE_0CC0);
    assert_eq!(scan.big(start + 20), 0xFADE_0C02);
    assert_eq!(scan.big(start + 28), 0x0002_0400);
    assert_eq!(raw[start + 59], 14);

    let alpha = scan.quad(0x4000);
    let bravo = scan.quad(0x4008);
    let charlie = scan.quad(0x4010);
    assert_ne!(alpha & (1u64 << 63), 0);
    assert_ne!(bravo & (1u64 << 63), 0);
    assert_eq!(charlie & (1u64 << 63), 0);
}

#[test]
fn shared() {
    let mut first = blank(Mode::Shared);
    first.name = "codec".into();
    first.code = vec![
        0x20, 0x00, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6, 0x40, 0x00, 0x80, 0x52, 0xc0, 0x03, 0x5f,
        0xd6, 0x60, 0x00, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6,
    ];
    first.data = vec![0u8; 16];
    first.symbols = vec![
        Symbol {
            name: "_sx$app$codec$encode".into(),
            offset: 0,
            size: 8,
            code: true,
            exported: true,
        },
        Symbol {
            name: "_sx$app$codec$decode".into(),
            offset: 8,
            size: 8,
            code: true,
            exported: true,
        },
        Symbol {
            name: "_sx$app$codec$secret".into(),
            offset: 16,
            size: 8,
            code: true,
            exported: false,
        },
        Symbol {
            name: "_sx$vt$app$codec$Stream".into(),
            offset: 0,
            size: 16,
            code: false,
            exported: true,
        },
    ];
    first.relocations = vec![Relocation {
        offset: 0,
        symbol: "_sx$app$codec$encode".into(),
        kind: Fixup::Pointer,
        addend: 0,
    }];

    let raw = emit(&first).expect("mach dylib");
    let scan = Scanner { bytes: &raw };
    assert_eq!(scan.word(12), 6);
    assert!(scan.find(0x0D).is_some());

    let (exports, _) = scan.find(0x8000_0033).expect("exports trie");
    let start = scan.word(exports + 8) as usize;
    let length = scan.word(exports + 12) as usize;
    let trie = &raw[start..start + length];

    let encode = lookup(trie, "_sx$app$codec$encode").expect("encode export");
    let decode = lookup(trie, "_sx$app$codec$decode").expect("decode export");
    let vtable = lookup(trie, "_sx$vt$app$codec$Stream").expect("vtable export");
    assert_eq!(decode, encode + 8);
    assert!(vtable >= 0x4000);
    assert_eq!(lookup(trie, "_sx$app$codec$secret"), None);

    let mut second = first.clone();
    second.code[0] = 0x80;
    let updated = emit(&second).expect("updated dylib");
    let rescan = Scanner { bytes: &updated };
    let (left, _) = scan.find(0x1B).unwrap();
    let (right, _) = rescan.find(0x1B).unwrap();
    assert_ne!(&raw[left + 8..left + 24], &updated[right + 8..right + 24]);
}
