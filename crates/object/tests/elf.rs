use simplex_object::{Artifact, Fault, Fixup, Format, Import, Mode, Relocation, Symbol, emit};
use std::collections::BTreeMap;

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn half(&self, offset: usize) -> u16 {
        u16::from_le_bytes(self.bytes[offset..offset + 2].try_into().unwrap())
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }

    fn signed(&self, offset: usize) -> i32 {
        i32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }

    fn quad(&self, offset: usize) -> u64 {
        u64::from_le_bytes(self.bytes[offset..offset + 8].try_into().unwrap())
    }

    fn text(&self, table: usize, offset: usize) -> String {
        let start = table + offset;
        let end = self.bytes[start..].iter().position(|&b| b == 0).unwrap() + start;
        String::from_utf8(self.bytes[start..end].to_vec()).unwrap()
    }

    fn tags(&self) -> BTreeMap<i64, Vec<u64>> {
        let count = usize::from(self.half(56));
        let mut map = BTreeMap::new();
        for index in 0..count {
            let pos = 64 + index * 56;
            if self.word(pos) == 2 {
                let start = self.quad(pos + 8) as usize;
                let size = self.quad(pos + 32) as usize;
                for cursor in (start..start + size).step_by(16) {
                    let tag = self.quad(cursor) as i64;
                    let value = self.quad(cursor + 8);
                    map.entry(tag).or_insert_with(Vec::new).push(value);
                }
            }
        }
        map
    }
}

fn blank(mode: Mode) -> Artifact {
    Artifact {
        format: Format::Elf,
        mode,
        name: "sample".into(),
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
    let empty = blank(Mode::Standalone);
    assert_eq!(emit(&empty), Err(Fault::Empty));

    let mut invalid = blank(Mode::Standalone);
    invalid.code = vec![0xc3];
    invalid.entry = 4;
    assert!(matches!(emit(&invalid), Err(Fault::Invalid(_))));

    invalid.entry = 0;
    invalid.relocations = vec![Relocation {
        offset: 0,
        symbol: "target".into(),
        kind: Fixup::Page,
        addend: 0,
    }];
    assert!(matches!(emit(&invalid), Err(Fault::Invalid(_))));

    invalid.code = vec![0xe8, 0, 0, 0, 0, 0xc3];
    invalid.imports.push(Import {
        name: "external".into(),
        library: "libc.so.6".into(),
        code: true,
    });
    invalid.relocations = vec![Relocation {
        offset: 1,
        symbol: "external".into(),
        kind: Fixup::Call,
        addend: -4,
    }];
    assert_eq!(emit(&invalid), Err(Fault::Unresolved("external".into())));
}

#[test]
fn standalone() {
    let mut item = blank(Mode::Standalone);
    item.code = vec![
        0xe8, 0, 0, 0, 0, 0x48, 0x8d, 0x35, 0, 0, 0, 0, 0x48, 0x8b, 0x05, 0, 0, 0, 0, 0xc3, 0xb8,
        7, 0, 0, 0, 0xc3,
    ];
    item.data = vec![0u8; 16];
    item.symbols = vec![
        Symbol {
            name: "helper".into(),
            offset: 20,
            size: 6,
            code: true,
            exported: false,
        },
        Symbol {
            name: "datum".into(),
            offset: 0,
            size: 8,
            code: false,
            exported: false,
        },
    ];
    item.relocations = vec![
        Relocation {
            offset: 1,
            symbol: "helper".into(),
            kind: Fixup::Call,
            addend: -4,
        },
        Relocation {
            offset: 8,
            symbol: "datum".into(),
            kind: Fixup::Relative,
            addend: -4,
        },
        Relocation {
            offset: 15,
            symbol: "datum".into(),
            kind: Fixup::Got,
            addend: -4,
        },
        Relocation {
            offset: 8,
            symbol: "helper".into(),
            kind: Fixup::Pointer,
            addend: 0,
        },
    ];

    let raw = emit(&item).expect("standalone elf");
    let reader = Reader { bytes: &raw };
    assert_eq!(&raw[0..4], b"\x7fELF");
    assert_eq!(reader.half(16), 2);
    assert_eq!(reader.quad(24), 0x0040_1000);
    let call = reader.signed(0x1001);
    assert_eq!(0x0040_1005i64 + i64::from(call), 0x0040_1014);
    assert_eq!(reader.quad(0x2008), 0x0040_1014);
}

#[test]
fn dynamic() {
    let mut item = blank(Mode::Dynamic);
    item.code = vec![0xe8, 0, 0, 0, 0, 0x48, 0x8b, 0x05, 0, 0, 0, 0, 0xc3];
    item.data = vec![0u8; 16];
    item.symbols.push(Symbol {
        name: "local".into(),
        offset: 12,
        size: 1,
        code: true,
        exported: false,
    });
    item.imports = vec![
        Import {
            name: "write".into(),
            library: "libc.so.6".into(),
            code: true,
        },
        Import {
            name: "table".into(),
            library: "libcodec.so".into(),
            code: false,
        },
    ];
    item.relocations = vec![
        Relocation {
            offset: 1,
            symbol: "write".into(),
            kind: Fixup::Call,
            addend: -4,
        },
        Relocation {
            offset: 8,
            symbol: "table".into(),
            kind: Fixup::Got,
            addend: -4,
        },
        Relocation {
            offset: 0,
            symbol: "local".into(),
            kind: Fixup::Pointer,
            addend: 0,
        },
    ];

    let raw = emit(&item).expect("dynamic elf");
    let reader = Reader { bytes: &raw };
    assert_eq!(reader.half(16), 3);
    let tags = reader.tags();
    let strings = tags[&5][0] as usize;
    let needed: Vec<String> = tags[&1]
        .iter()
        .map(|&off| reader.text(strings, off as usize))
        .collect();
    assert_eq!(needed, vec!["libc.so.6", "libcodec.so"]);
    assert_eq!(reader.text(strings, tags[&29][0] as usize), "$ORIGIN");
    let call = reader.signed(0x1001);
    let stub = (0x1005i64 + i64::from(call)) as usize;
    assert_eq!(&raw[stub..stub + 2], &[0xff, 0x25]);
}

#[test]
fn shared() {
    let mut item = blank(Mode::Shared);
    item.code = vec![0xb8, 1, 0, 0, 0, 0xc3, 0xb8, 2, 0, 0, 0, 0xc3];
    item.data = vec![0u8; 8];
    item.symbols = vec![
        Symbol {
            name: "hidden".into(),
            offset: 0,
            size: 6,
            code: true,
            exported: false,
        },
        Symbol {
            name: "visible".into(),
            offset: 6,
            size: 6,
            code: true,
            exported: true,
        },
    ];
    item.relocations.push(Relocation {
        offset: 0,
        symbol: "hidden".into(),
        kind: Fixup::Pointer,
        addend: 0,
    });

    let raw = emit(&item).expect("shared elf");
    let reader = Reader { bytes: &raw };
    let tags = reader.tags();
    let strings = tags[&5][0] as usize;
    let symbols = tags[&6][0] as usize;
    assert_eq!(reader.word(tags[&4][0] as usize + 4), 2);
    assert_eq!(
        reader.text(strings, reader.word(symbols + 24) as usize),
        "visible"
    );
}
