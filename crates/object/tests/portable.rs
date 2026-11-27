use simplex_object::{Artifact, Fault, Fixup, Format, Import, Mode, Relocation, Symbol, emit};

struct Inspector<'a> {
    bytes: &'a [u8],
}

impl<'a> Inspector<'a> {
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

    fn header(&self) -> usize {
        self.word(0x3c) as usize
    }

    fn optional(&self) -> usize {
        self.header() + 24
    }

    fn directory(&self, index: usize) -> (u32, u32) {
        let position = self.optional() + 112 + index * 8;
        (self.word(position), self.word(position + 4))
    }

    fn translate(&self, address: u32) -> usize {
        let count = usize::from(self.half(self.header() + 6));
        let start = self.optional() + usize::from(self.half(self.header() + 20));
        for index in 0..count {
            let position = start + index * 40;
            let span = self.word(position + 8);
            let base = self.word(position + 12);
            let raw = self.word(position + 16);
            let offset = self.word(position + 20);
            let limit = span.max(raw);
            if address >= base && address < base + limit {
                return (offset + (address - base)) as usize;
            }
        }
        panic!("unmapped rva {address:#x}");
    }

    fn string(&self, address: u32) -> String {
        let start = self.translate(address);
        let end = self.bytes[start..].iter().position(|&b| b == 0).unwrap() + start;
        String::from_utf8(self.bytes[start..end].to_vec()).unwrap()
    }
}

fn blank(mode: Mode) -> Artifact {
    Artifact {
        format: Format::Portable,
        mode,
        name: "probe".into(),
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
    assert_eq!(emit(&blank(Mode::Dynamic)), Err(Fault::Empty));

    let mut missing = blank(Mode::Dynamic);
    missing.code = vec![0xc3];
    missing.imports.push(Import {
        name: "ExitProcess".into(),
        library: String::new(),
        code: true,
    });
    assert!(matches!(emit(&missing), Err(Fault::Invalid(_))));

    let mut bad = blank(Mode::Dynamic);
    bad.code = vec![0xc3];
    bad.relocations.push(Relocation {
        offset: 0,
        symbol: "none".into(),
        kind: Fixup::Call,
        addend: -4,
    });
    assert!(matches!(emit(&bad), Err(Fault::Invalid(_))));

    bad.code = vec![0xe8, 0, 0, 0, 0, 0xc3];
    bad.relocations[0].offset = 1;
    assert_eq!(emit(&bad), Err(Fault::Unresolved("none".into())));
}

#[test]
fn executable() {
    let mut item = blank(Mode::Dynamic);
    item.code = vec![
        0xe8, 0, 0, 0, 0, 0x48, 0x8b, 0x05, 0, 0, 0, 0, 0xc3, 0xb8, 9, 0, 0, 0, 0xc3,
    ];
    item.data = vec![0u8; 8];
    item.symbols.push(Symbol {
        name: "method".into(),
        offset: 13,
        size: 6,
        code: true,
        exported: false,
    });
    item.imports = vec![
        Import {
            name: "_write".into(),
            library: "ucrtbase.dll".into(),
            code: true,
        },
        Import {
            name: "malloc".into(),
            library: "ucrtbase.dll".into(),
            code: true,
        },
        Import {
            name: "ExitProcess".into(),
            library: "KERNEL32.dll".into(),
            code: true,
        },
        Import {
            name: "sx$vt$codec".into(),
            library: "codec.dll".into(),
            code: false,
        },
    ];
    item.relocations = vec![
        Relocation {
            offset: 1,
            symbol: "_write".into(),
            kind: Fixup::Call,
            addend: -4,
        },
        Relocation {
            offset: 8,
            symbol: "sx$vt$codec".into(),
            kind: Fixup::Got,
            addend: -4,
        },
        Relocation {
            offset: 0,
            symbol: "method".into(),
            kind: Fixup::Pointer,
            addend: 0,
        },
    ];

    let raw = emit(&item).expect("portable executable");
    let view = Inspector { bytes: &raw };
    let mark = view.header();
    assert_eq!(&raw[0..2], b"MZ");
    assert_eq!(&raw[mark..mark + 4], b"PE\0\0");
    assert_eq!(view.half(mark + 4), 0x8664);
    assert_eq!(view.half(mark + 22) & 0x2000, 0);
    assert_eq!(view.half(view.optional()), 0x020B);
    assert_eq!(view.quad(view.optional() + 24), 0x0001_4000_0000);
    assert_eq!(view.half(view.optional() + 68), 3);

    let text = view.translate(0x1000);
    let delta = view.signed(text + 1);
    let jump = (0x1005i64 + i64::from(delta)) as u32;
    let thunk = view.translate(jump);
    assert_eq!(&raw[thunk..thunk + 2], &[0xFF, 0x25]);

    let (imports, width) = view.directory(1);
    assert_eq!(width, 80);
    let descriptors = view.translate(imports);
    let libraries: Vec<String> = (0..3)
        .map(|index| view.string(view.word(descriptors + index * 20 + 12)))
        .collect();
    assert_eq!(libraries, vec!["ucrtbase.dll", "KERNEL32.dll", "codec.dll"]);

    let storage = view.translate(0x2000);
    assert_eq!(view.quad(storage), 0x0001_4000_100D);
    let (fixes, span) = view.directory(5);
    assert!(span >= 12);
    let block = view.translate(fixes);
    assert_eq!(view.word(block), 0x2000);
    assert_eq!(view.half(block + 8), 0xA000);
}

#[test]
fn shared() {
    let mut item = blank(Mode::Shared);
    item.name = "codec".into();
    item.code = vec![
        0xb8, 10, 0, 0, 0, 0xc3, 0xb8, 20, 0, 0, 0, 0xc3, 0xb8, 30, 0, 0, 0, 0xc3,
    ];
    item.symbols = vec![
        Symbol {
            name: "sx$zeta".into(),
            offset: 0,
            size: 6,
            code: true,
            exported: true,
        },
        Symbol {
            name: "sx$private".into(),
            offset: 6,
            size: 6,
            code: true,
            exported: false,
        },
        Symbol {
            name: "sx$alpha".into(),
            offset: 12,
            size: 6,
            code: true,
            exported: true,
        },
    ];

    let raw = emit(&item).expect("portable dll");
    let view = Inspector { bytes: &raw };
    let mark = view.header();
    assert_ne!(view.half(mark + 22) & 0x2000, 0);
    assert_eq!(view.quad(view.optional() + 24), 0x0001_8000_0000);

    let entry = view.word(view.optional() + 16);
    let start = view.translate(entry);
    assert_eq!(&raw[start..start + 6], &[0xB8, 1, 0, 0, 0, 0xC3]);

    let (exports, _) = view.directory(0);
    let table = view.translate(exports);
    assert_eq!(view.string(view.word(table + 12)), "codec.dll");
    assert_eq!(view.word(table + 20), 2);
    assert_eq!(view.word(table + 24), 2);
    let functions = view.translate(view.word(table + 28));
    let names = view.translate(view.word(table + 32));
    let ordinals = view.translate(view.word(table + 36));
    assert_eq!(view.string(view.word(names)), "sx$alpha");
    assert_eq!(view.string(view.word(names + 4)), "sx$zeta");
    let first = usize::from(view.half(ordinals));
    let second = usize::from(view.half(ordinals + 2));
    assert_eq!(view.word(functions + first * 4), 0x100C);
    assert_eq!(view.word(functions + second * 4), 0x1000);
}
