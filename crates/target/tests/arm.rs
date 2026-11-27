use simplex_ir::{
    Asset, Block, External, Function, Instruction, Linkage, Module, Operation, Table, Terminator,
    Type,
};
use simplex_object::{Fixup, Format, Mode};
use simplex_target::arm::{Encoder, Width};
use simplex_target::{Platform, compile};

fn words(bytes: &[u8]) -> Vec<u32> {
    assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

const fn word() -> Type {
    Type::Integer {
        bits: 64,
        signed: true,
    }
}

fn step(value: u32, kind: Type, op: Operation) -> Instruction {
    Instruction { value, kind, op }
}

fn block(id: u32, params: Vec<(u32, Type)>, body: Vec<Instruction>, exit: Terminator) -> Block {
    Block {
        id,
        parameters: params,
        body,
        exit,
    }
}

fn routine(symbol: &str, inputs: Vec<(u32, Type)>, output: Type, blocks: Vec<Block>) -> Function {
    Function {
        symbol: symbol.to_owned(),
        linkage: Linkage::Exported,
        asynchronous: false,
        cleanup: None,
        inputs,
        output,
        values: 32,
        blocks,
    }
}

fn import(symbol: &str, linkage: Linkage, inputs: Vec<Type>, output: Type) -> External {
    External {
        symbol: symbol.to_owned(),
        linkage,
        inputs,
        output,
    }
}

#[test]
fn encoding() {
    let mut enc = Encoder::new();
    enc.enter();
    enc.address(0, "_asset", false);
    enc.address(1, "_imported", true);
    enc.call("_helper");
    enc.indirect(16);
    enc.fence();
    enc.acquire(0, 1, Width::Quad);
    enc.release(0, 1, Width::Quad);
    enc.exclusive(0, 1, Width::Quad);
    enc.commit(2, (0, 1), Width::Quad);
    let label = enc.fresh();
    enc.zero(0, false, label);
    enc.trap();
    enc.bind(label);
    enc.exit();
    let (raw, relocs) = enc.finish().expect("encode");
    let stream = words(&raw);
    assert_eq!(stream[0..2], [0xA9BF_7BFD, 0x9100_03FD]);
    for op in [
        0xD420_0020,
        0xD503_3BBF,
        0xC8DF_FC20,
        0xC89F_FC20,
        0xC85F_FC20,
        0xC802_FC20,
    ] {
        assert!(stream.contains(&op));
    }
    assert_eq!(*stream.last().expect("ret"), 0xD65F_03C0);
    assert_eq!(relocs[0].kind, Fixup::Page);
    assert_eq!(relocs[1].kind, Fixup::Low { scale: 0 });
    assert_eq!(relocs[3].kind, Fixup::Low { scale: 3 });
    assert_eq!(relocs[4].kind, Fixup::Call);
}

#[test]
fn conventions() {
    let big = Type::Aggregate {
        size: 24,
        align: 8,
        packed: false,
    };
    let mid = Type::Aggregate {
        size: 12,
        align: 4,
        packed: false,
    };
    let sig = vec![
        (0, word()),
        (1, big.clone()),
        (2, mid.clone()),
        (3, Type::Pair),
    ];
    let free = Operation::Call {
        symbol: String::from("sx$rt$free"),
        arguments: vec![4, 0],
        inputs: vec![Type::Pointer, word()],
        kind: Type::Void,
    };
    let b0 = block(
        0,
        Vec::new(),
        vec![
            step(
                4,
                Type::Pointer,
                Operation::Low {
                    pair: 3,
                    kind: Type::Pointer,
                },
            ),
            step(5, Type::Void, free),
        ],
        Terminator::Return(Some(1)),
    );
    let callee = routine("sx$a$b$build", sig, big.clone(), vec![b0]);
    let call = Operation::Call {
        symbol: String::from("sx$a$b$build"),
        arguments: vec![5, 1, 2, 4],
        inputs: vec![word(), big.clone(), mid.clone(), Type::Pair],
        kind: big.clone(),
    };
    let c0 = block(
        0,
        Vec::new(),
        vec![
            step(
                0,
                Type::Pointer,
                Operation::Slot {
                    size: 24,
                    align: 16,
                },
            ),
            step(
                1,
                big.clone(),
                Operation::Load {
                    address: 0,
                    kind: big.clone(),
                    volatile: false,
                },
            ),
            step(
                2,
                mid.clone(),
                Operation::Load {
                    address: 0,
                    kind: mid,
                    volatile: false,
                },
            ),
            step(
                3,
                Type::Pointer,
                Operation::Symbol {
                    name: String::from("sx$vt$a$codec$C$D"),
                },
            ),
            step(4, Type::Pair, Operation::Pair { low: 0, high: 3 }),
            step(
                5,
                word(),
                Operation::Integer {
                    value: 42,
                    kind: word(),
                },
            ),
            step(6, big.clone(), call),
            step(
                7,
                Type::Void,
                Operation::Store {
                    address: 0,
                    value: 6,
                    kind: big,
                    volatile: false,
                },
            ),
            step(
                8,
                word(),
                Operation::Load {
                    address: 0,
                    kind: word(),
                    volatile: true,
                },
            ),
        ],
        Terminator::Return(Some(8)),
    );
    let caller = routine("sx$a$b$main", Vec::new(), word(), vec![c0]);
    let module = Module {
        shared: false,
        entry: Some(String::from("sx$a$b$main")),
        assets: vec![Asset {
            symbol: String::from("sx$ro$a$b$0"),
            bytes: b"hi".to_vec(),
        }],
        tables: vec![Table {
            symbol: String::from("sx$vt$a$b$X$Y"),
            methods: vec![String::from("sx$a$b$build")],
        }],
        externals: vec![
            import("sx$rt$alloc", Linkage::System, vec![word()], Type::Pointer),
            import(
                "sx$rt$free",
                Linkage::System,
                vec![Type::Pointer, word()],
                Type::Void,
            ),
            import(
                "write",
                Linkage::System,
                vec![word(), Type::Pointer, word()],
                word(),
            ),
            import(
                "sx$a$codec$run",
                Linkage::Dynamic(String::from("codec")),
                vec![Type::Pair],
                word(),
            ),
        ],
        functions: vec![callee, caller],
    };
    let out = compile(Platform::Apple, "demo", &module).expect("compile");
    assert_eq!((out.format, out.mode), (Format::Mach, Mode::Dynamic));
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "_main" && s.offset == out.entry)
    );
    for name in [
        "_malloc",
        "_free",
        "_write",
        "_sx$a$codec$run",
        "_sx$vt$a$codec$C$D",
    ] {
        assert!(out.imports.iter().any(|i| i.name == name));
    }
    let want = "@executable_path/codec.dylib";
    assert!(out.imports.iter().any(|i| i.library == want));
    assert!(
        words(&out.code)
            .iter()
            .any(|&w| (w & 0xFF00_03FF) == (0x9100_0000 | (31 << 5) | 8))
    );
}
