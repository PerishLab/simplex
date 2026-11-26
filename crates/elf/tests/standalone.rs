use simplex_elf::{BASE, Fault, Kind, Mode, PAGE, Plan, Relocation, Symbol, write};

struct View<'a> {
    bytes: &'a [u8],
}

impl<'a> View<'a> {
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

    fn count(&self) -> usize {
        usize::from(self.half(56))
    }

    fn base(&self, index: usize) -> usize {
        64 + index * 56
    }
}

#[test]
fn modes() {
    assert_eq!(Mode::select(false, false), Mode::Standalone);
    assert_eq!(Mode::select(false, true), Mode::Dynamic);
    assert_eq!(Mode::select(true, false), Mode::Shared);
    assert_eq!(Mode::select(true, true), Mode::Shared);
}

#[test]
fn errors() {
    let empty = Plan::standalone(Vec::new(), Vec::new());
    assert_eq!(write(&empty), Err(Fault::Empty));
    assert_eq!(Fault::Empty.to_string(), "empty machine code section");
    assert_eq!(
        Fault::Unresolved("missing".into()).to_string(),
        "unresolved symbol: missing"
    );
    assert!(
        Fault::Unsupported(Mode::Shared)
            .to_string()
            .contains("Shared")
    );

    let mut broken = Plan::standalone(vec![0xe8, 0, 0, 0, 0, 0xc3], Vec::new());
    broken.imports.push("external".into());
    for kind in [
        Kind::Relative32,
        Kind::Procedure32,
        Kind::Global32,
        Kind::Absolute64,
    ] {
        broken.data = vec![0u8; 8];
        broken.relocations = vec![Relocation {
            offset: 0,
            symbol: "external".into(),
            kind,
            addend: -4,
        }];
        assert_eq!(write(&broken), Err(Fault::Unresolved("external".into())));
    }
}

#[test]
fn minimal() {
    let mut plan = Plan::standalone(vec![0xb8, 0x2a, 0, 0, 0, 0xc3], Vec::new());
    plan.entry = 0;
    let raw = write(&plan).expect("standalone image");
    let view = View { bytes: &raw };

    assert_eq!(&raw[0..4], b"\x7fELF");
    assert_eq!(raw[4], 2);
    assert_eq!(raw[5], 1);
    assert_eq!(raw[6], 1);
    assert_eq!(view.half(16), 2);
    assert_eq!(view.half(18), 62);
    assert_eq!(view.quad(24), BASE + PAGE);
    assert_eq!(view.quad(32), 64);
    assert_eq!(view.half(52), 64);
    assert_eq!(view.half(54), 56);

    let mut loads = Vec::new();
    for index in 0..view.count() {
        let pos = view.base(index);
        let kind = view.word(pos);
        assert_ne!(kind, 2);
        assert_ne!(kind, 3);
        if kind == 1 {
            let offset = view.quad(pos + 8);
            let address = view.quad(pos + 16);
            assert_eq!(offset % PAGE, address % PAGE);
            assert_eq!(view.quad(pos + 48), PAGE);
            loads.push((address, view.word(pos + 4)));
        }
    }
    assert_eq!(loads, vec![(BASE, 4), (BASE + PAGE, 5)]);
    assert_eq!(
        &raw[PAGE as usize..PAGE as usize + 6],
        &[0xb8, 0x2a, 0, 0, 0, 0xc3]
    );
}

#[test]
fn relocations() {
    let code = vec![
        0xe8, 0, 0, 0, 0, 0x48, 0x8d, 0x35, 0, 0, 0, 0, 0x48, 0x8b, 0x05, 0, 0, 0, 0, 0xe8, 0, 0,
        0, 0, 0xc3, 0xb8, 7, 0, 0, 0, 0xc3,
    ];
    let mut data = b"hello\0\0\0".to_vec();
    data.extend_from_slice(&[0u8; 8]);
    let plan = Plan {
        mode: Mode::Standalone,
        entry: 0,
        needed: Vec::new(),
        code,
        data,
        exports: vec![
            Symbol {
                name: "helper".into(),
                offset: 25,
                size: 6,
                code: true,
            },
            Symbol {
                name: "greeting".into(),
                offset: 0,
                size: 5,
                code: false,
            },
            Symbol {
                name: "slot".into(),
                offset: 8,
                size: 8,
                code: false,
            },
        ],
        imports: Vec::new(),
        relocations: vec![
            Relocation {
                offset: 1,
                symbol: "helper".into(),
                kind: Kind::Relative32,
                addend: -4,
            },
            Relocation {
                offset: 8,
                symbol: "greeting".into(),
                kind: Kind::Relative32,
                addend: -4,
            },
            Relocation {
                offset: 15,
                symbol: "slot".into(),
                kind: Kind::Global32,
                addend: -4,
            },
            Relocation {
                offset: 20,
                symbol: "helper".into(),
                kind: Kind::Procedure32,
                addend: -4,
            },
            Relocation {
                offset: 8,
                symbol: "helper".into(),
                kind: Kind::Absolute64,
                addend: 0,
            },
        ],
    };

    let raw = write(&plan).expect("relocated standalone");
    let view = View { bytes: &raw };
    let text = PAGE as usize;
    let first = view.signed(text + 1);
    assert_eq!(
        (BASE + PAGE + 5) as i64 + i64::from(first),
        (BASE + PAGE + 25) as i64
    );

    let second = view.signed(text + 8);
    assert_eq!(
        (BASE + PAGE + 12) as i64 + i64::from(second),
        (BASE + 2 * PAGE) as i64
    );

    let third = view.signed(text + 15);
    let entry = ((BASE + PAGE + 19) as i64 + i64::from(third)) as u64;
    let file = (entry - BASE) as usize;
    assert_eq!(view.quad(file), BASE + 2 * PAGE + 8);

    let fourth = view.signed(text + 20);
    assert_eq!(
        (BASE + PAGE + 24) as i64 + i64::from(fourth),
        (BASE + PAGE + 25) as i64
    );

    let pointer = (2 * PAGE + 8) as usize;
    assert_eq!(view.quad(pointer), BASE + PAGE + 25);
}
