use simplex_elf::{INTERPRETER, Kind, Mode, PAGE, Plan, Relocation, Symbol, write};
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

    fn dynamic(&self) -> BTreeMap<i64, Vec<u64>> {
        let count = usize::from(self.half(56));
        let mut map = BTreeMap::new();
        for index in 0..count {
            let pos = 64 + index * 56;
            if self.word(pos) == 2 {
                let start = self.quad(pos + 8) as usize;
                let size = self.quad(pos + 32) as usize;
                for cursor in (start..start + size).step_by(16) {
                    let tag = self.quad(cursor) as i64;
                    let val = self.quad(cursor + 8);
                    map.entry(tag).or_insert_with(Vec::new).push(val);
                }
            }
        }
        map
    }
}

fn sample() -> Plan {
    let code = vec![0xe8, 0, 0, 0, 0, 0x48, 0x8b, 0x05, 0, 0, 0, 0, 0xc3];
    Plan {
        mode: Mode::Dynamic,
        entry: 0,
        needed: vec!["libc.so.6".into(), "libcodec.so".into()],
        code,
        data: vec![0u8; 16],
        exports: vec![Symbol {
            name: "local".into(),
            offset: 12,
            size: 1,
            code: true,
        }],
        imports: vec!["write".into(), "table".into(), "datum".into()],
        relocations: vec![
            Relocation {
                offset: 1,
                symbol: "write".into(),
                kind: Kind::Procedure32,
                addend: -4,
            },
            Relocation {
                offset: 8,
                symbol: "table".into(),
                kind: Kind::Global32,
                addend: -4,
            },
            Relocation {
                offset: 0,
                symbol: "local".into(),
                kind: Kind::Absolute64,
                addend: 0,
            },
            Relocation {
                offset: 8,
                symbol: "datum".into(),
                kind: Kind::Absolute64,
                addend: 4,
            },
        ],
    }
}

#[test]
fn headers() {
    let raw = write(&sample()).expect("dynamic executable");
    let reader = Reader { bytes: &raw };
    assert_eq!(reader.half(16), 3);
    assert_eq!(reader.quad(24), PAGE);

    let count = usize::from(reader.half(56));
    let mut found = false;
    let mut loads = 0usize;
    for index in 0..count {
        let pos = 64 + index * 56;
        match reader.word(pos) {
            3 => {
                found = true;
                let start = reader.quad(pos + 8) as usize;
                let size = reader.quad(pos + 32) as usize;
                assert_eq!(&raw[start..start + size - 1], INTERPRETER.as_bytes());
                assert_eq!(raw[start + size - 1], 0);
            }
            1 => {
                loads += 1;
                assert_eq!(reader.quad(pos + 8), reader.quad(pos + 16));
                assert_eq!(reader.quad(pos + 48), PAGE);
            }
            _ => {}
        }
    }
    assert!(found);
    assert_eq!(loads, 3);
}

#[test]
fn linkage() {
    let raw = write(&sample()).expect("dynamic linkage");
    let reader = Reader { bytes: &raw };
    let tags = reader.dynamic();
    let strings = tags[&5][0] as usize;
    let symbols = tags[&6][0] as usize;

    let needed: Vec<String> = tags[&1]
        .iter()
        .map(|&off| reader.text(strings, off as usize))
        .collect();
    assert_eq!(needed, vec!["libc.so.6", "libcodec.so"]);
    assert_eq!(reader.text(strings, tags[&29][0] as usize), "$ORIGIN");
    assert_eq!(tags[&11][0], 24);
    assert_eq!(tags[&0x6fff_fffb][0], 0x0800_0000);
    assert_eq!(tags[&0][0], 0);

    let call = reader.signed(PAGE as usize + 1);
    let stub = (PAGE as i64 + 5 + i64::from(call)) as usize;
    assert_eq!(&raw[stub..stub + 2], &[0xff, 0x25]);
    assert_eq!(raw[stub + 6], 0x68);
    assert_eq!(reader.word(stub + 7), 0);

    let pltgot = tags[&3][0] as usize;
    let slot = pltgot + 24;
    assert_eq!(reader.quad(slot), (stub + 6) as u64);

    let jmprel = tags[&23][0] as usize;
    assert_eq!(reader.quad(jmprel), slot as u64);
    let info = reader.quad(jmprel + 8);
    assert_eq!(info & 0xffff_ffff, 7);
    let sym = symbols + ((info >> 32) as usize) * 24;
    assert_eq!(reader.text(strings, reader.word(sym) as usize), "write");

    let rela = tags[&7][0] as usize;
    let span = tags[&8][0] as usize;
    assert_eq!(span, 72);
    let kinds: Vec<u32> = (0..3)
        .map(|idx| (reader.quad(rela + idx * 24 + 8) & 0xffff_ffff) as u32)
        .collect();
    assert_eq!(kinds, vec![8, 6, 1]);
}
