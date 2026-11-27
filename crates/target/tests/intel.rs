use simplex_ir::{
    Asset, Atomic, Binary, Block, Cast, External, Function, Instruction, Linkage, Module,
    Operation, Order, Table, Target, Terminator, Type,
};
use simplex_object::{Fixup, Format, Mode};
use simplex_target::allocate::assign;
use simplex_target::intel::encode::Encoder;
use simplex_target::intel::{Address, Register, Width};
use simplex_target::machine::{Bank, Location, Span, stretch};
use simplex_target::{Platform, compile};

fn int(bits: u8, signed: bool) -> Type {
    Type::Integer { bits, signed }
}

fn span(id: u32, bounds: (u32, u32), float: bool) -> Span {
    Span {
        value: id,
        start: bounds.0,
        end: bounds.1,
        float,
        across: false,
    }
}

fn external(symbol: &str, linkage: Linkage, inputs: Vec<Type>, output: Type) -> External {
    External {
        symbol: symbol.to_owned(),
        linkage,
        inputs,
        output,
    }
}

#[derive(Default)]
struct Steps {
    body: Vec<Instruction>,
}

impl Steps {
    fn push(&mut self, id: u32, kind: Type, op: Operation) {
        self.body.push(Instruction {
            value: id,
            kind,
            op,
        });
    }

    fn constant(&mut self, id: u32, val: i128, kind: Type) {
        let op = Operation::Integer {
            value: val,
            kind: kind.clone(),
        };
        self.push(id, kind, op);
    }

    fn call(&mut self, id: u32, sym: &str, args: (Vec<u32>, Vec<Type>), kind: Type) {
        let op = Operation::Call {
            symbol: sym.to_owned(),
            arguments: args.0,
            inputs: args.1,
            kind: kind.clone(),
        };
        self.push(id, kind, op);
    }

    fn block(self, id: u32, exit: Terminator) -> Block {
        Block {
            id,
            parameters: Vec::new(),
            body: self.body,
            exit,
        }
    }
}

fn routine(
    symbol: &str,
    exported: bool,
    sig: (Vec<(u32, Type)>, Type),
    blocks: Vec<Block>,
) -> Function {
    let linkage = if exported {
        Linkage::Exported
    } else {
        Linkage::Internal
    };
    Function {
        symbol: symbol.to_owned(),
        linkage,
        asynchronous: false,
        cleanup: None,
        inputs: sig.0,
        output: sig.1,
        values: 16,
        blocks,
    }
}

#[test]
fn allocator() {
    let bank = Bank {
        general: vec![8, 9, 3, 12],
        preserved: vec![3, 12],
        float: vec![1, 2],
    };
    let mut spans = vec![
        span(0, (1, 4), false),
        span(1, (2, 3), false),
        span(2, (1, 8), true),
        span(3, (5, 7), true),
    ];
    stretch(&mut spans, &[(2, 6)], &[5]);
    assert!(spans[0].across && !spans[1].across && spans[2].across);
    let plan = assign(&bank, &spans);
    assert_eq!(plan.locations.get(&0), Some(&Location::General(3)));
    assert_eq!(plan.locations.get(&1), Some(&Location::General(8)));
    assert_eq!(plan.locations.get(&2), Some(&Location::Spill(0)));
    assert_eq!(plan.locations.get(&3), Some(&Location::Float(1)));
    assert_eq!((plan.used, plan.frame), (vec![3], 16));
}

#[test]
fn encodings() {
    let mut enc = Encoder::new();
    enc.fence();
    enc.swap(Address::new(Register::R10, 0), Register::Rcx, Width::Quad);
    enc.accumulate(Address::new(Register::R10, 8), Register::Rax, Width::Double);
    enc.trap();
    let (bytes, relocs) = enc.finish().expect("encoding");
    assert!(relocs.is_empty());
    assert_eq!(&bytes[0..3], &[0x0F, 0xAE, 0xF0]);
    assert_eq!(&bytes[3..8], &[0xF0, 0x49, 0x0F, 0xB1, 0x0A]);
    assert_eq!(&bytes[8..14], &[0xF0, 0x41, 0x0F, 0xC1, 0x42, 0x08]);
    assert_eq!(&bytes[14..16], &[0x0F, 0x0B]);
}

#[test]
fn standalone() {
    let i64t = int(64, true);
    let mut b0 = Steps::default();
    b0.call(1, "sx$main", (vec![], vec![]), i64t.clone());
    b0.constant(2, 0, i64t.clone());
    b0.push(
        3,
        int(1, false),
        Operation::Binary {
            op: Binary::Eq,
            left: 1,
            right: 2,
            kind: i64t.clone(),
        },
    );
    let mut b1 = Steps::default();
    b1.call(
        4,
        "sx$rt$free",
        (vec![0, 1], vec![Type::Pointer, i64t.clone()]),
        Type::Void,
    );
    let exit0 = Terminator::Branch {
        condition: 3,
        truthy: Target {
            block: 0,
            arguments: vec![],
        },
        falsy: Target {
            block: 1,
            arguments: vec![],
        },
    };
    let drive = routine(
        "sx$drive",
        false,
        (vec![(0, Type::Pointer)], Type::Void),
        vec![b0.block(0, exit0), b1.block(1, Terminator::Return(None))],
    );
    let mut m0 = Steps::default();
    m0.call(0, "sx$rt$alloc", (vec![], vec![]), Type::Pointer);
    m0.call(1, "sx$drive", (vec![0], vec![Type::Pointer]), Type::Void);
    m0.constant(2, 42, i64t.clone());
    let root = routine(
        "sx$main",
        false,
        (Vec::new(), i64t),
        vec![m0.block(0, Terminator::Return(Some(2)))],
    );
    let module = Module {
        shared: false,
        entry: Some("sx$main".to_owned()),
        assets: vec![Asset {
            symbol: "sx$asset".to_owned(),
            bytes: b"hi".to_vec(),
        }],
        tables: vec![Table {
            symbol: "sx$table".to_owned(),
            methods: vec!["sx$main".to_owned()],
        }],
        externals: vec![external(
            "sx$rt$alloc",
            Linkage::System,
            vec![int(64, false)],
            Type::Pointer,
        )],
        functions: vec![drive, root],
    };
    let art = compile(Platform::Linux, "demo", &module).expect("linux");
    assert_eq!((art.format, art.mode), (Format::Elf, Mode::Standalone));
    let drv = art.symbols.iter().find(|s| s.name == "sx$drive").unwrap();
    assert_eq!(art.code[drv.offset as usize + 4], 0x53);
    assert!(art.relocations.iter().any(|r| r.kind == Fixup::Pointer));
}

#[test]
fn portable() {
    let big = Type::Aggregate {
        size: 24,
        align: 8,
        packed: false,
    };
    let f64t = Type::Float { bits: 64 };
    let i32t = int(32, true);
    let mut h0 = Steps::default();
    h0.push(3, Type::Pointer, Operation::Slot { size: 16, align: 8 });
    h0.push(
        4,
        i32t.clone(),
        Operation::Cast {
            op: Cast::Truncate,
            value: 2,
            from: f64t.clone(),
            kind: i32t.clone(),
        },
    );
    h0.push(
        5,
        i32t.clone(),
        Operation::Atomic {
            op: Atomic::Add,
            address: 3,
            operand: 4,
            kind: i32t.clone(),
            order: Order::Sequential,
        },
    );
    let helper = routine(
        "sx$exported",
        true,
        (vec![(0, big.clone()), (1, Type::Pair), (2, f64t)], big),
        vec![h0.block(0, Terminator::Return(Some(0)))],
    );
    let mut e0 = Steps::default();
    e0.constant(0, 1, i32t.clone());
    e0.call(1, "write", (vec![0], vec![i32t.clone()]), i32t.clone());
    e0.call(2, "sx$ext", (vec![1], vec![i32t.clone()]), i32t.clone());
    let entry = routine(
        "sx$main",
        false,
        (Vec::new(), i32t.clone()),
        vec![e0.block(0, Terminator::Return(Some(2)))],
    );
    let module = Module {
        shared: false,
        entry: Some("sx$main".to_owned()),
        assets: Vec::new(),
        tables: Vec::new(),
        externals: vec![
            external("write", Linkage::System, vec![i32t.clone()], i32t.clone()),
            external(
                "sx$ext",
                Linkage::Dynamic("codec".to_owned()),
                vec![i32t.clone()],
                i32t,
            ),
            external(
                "sx$rt$alloc",
                Linkage::System,
                vec![int(64, false)],
                Type::Pointer,
            ),
        ],
        functions: vec![helper, entry],
    };
    let exe = compile(Platform::Windows, "app", &module).expect("windows");
    assert_eq!((exe.format, exe.mode), (Format::Portable, Mode::Dynamic));
    assert!(exe.imports.iter().any(|i| i.name == "ExitProcess"));
    assert!(exe.imports.iter().any(|i| i.name == "_write"));
    assert!(exe.imports.iter().any(|i| i.name == "_setmode"));
    assert!(exe.imports.iter().any(|i| i.name == "malloc"));
    assert!(exe.imports.iter().any(|i| i.library == "codec.dll"));
}
