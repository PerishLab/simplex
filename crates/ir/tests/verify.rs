use simplex_ir::{
    Atomic, Binary, Block, External, Function, Instruction, Linkage, Module, Operation, Order,
    Target, Terminator, Type, verify,
};

const WORD: Type = Type::Integer {
    bits: 64,
    signed: true,
};
const FLAG: Type = Type::Integer {
    bits: 1,
    signed: false,
};

fn sample() -> Module {
    Module {
        shared: false,
        entry: Some("main".to_owned()),
        assets: Vec::new(),
        tables: Vec::new(),
        externals: vec![External {
            symbol: "ext".to_owned(),
            linkage: Linkage::System,
            inputs: vec![WORD],
            output: WORD,
        }],
        functions: vec![Function {
            symbol: "main".to_owned(),
            linkage: Linkage::Exported,
            asynchronous: false,
            cleanup: None,
            inputs: Vec::new(),
            output: WORD,
            values: 14,
            blocks: vec![
                Block {
                    id: 0,
                    parameters: Vec::new(),
                    body: vec![
                        Instruction {
                            value: 0,
                            kind: WORD,
                            op: Operation::Integer {
                                value: 10,
                                kind: WORD,
                            },
                        },
                        Instruction {
                            value: 1,
                            kind: WORD,
                            op: Operation::Integer {
                                value: 20,
                                kind: WORD,
                            },
                        },
                        Instruction {
                            value: 2,
                            kind: FLAG,
                            op: Operation::Binary {
                                op: Binary::Lt,
                                left: 0,
                                right: 1,
                                kind: WORD,
                            },
                        },
                        Instruction {
                            value: 3,
                            kind: Type::Pointer,
                            op: Operation::Slot { size: 8, align: 8 },
                        },
                        Instruction {
                            value: 4,
                            kind: Type::Void,
                            op: Operation::Store {
                                address: 3,
                                value: 0,
                                kind: WORD,
                                volatile: false,
                            },
                        },
                        Instruction {
                            value: 5,
                            kind: WORD,
                            op: Operation::Atomic {
                                op: Atomic::Add,
                                address: 3,
                                operand: 1,
                                kind: WORD,
                                order: Order::Sequential,
                            },
                        },
                    ],
                    exit: Terminator::Branch {
                        condition: 2,
                        truthy: Target {
                            block: 1,
                            arguments: vec![5],
                        },
                        falsy: Target {
                            block: 2,
                            arguments: vec![1],
                        },
                    },
                },
                Block {
                    id: 1,
                    parameters: vec![(6, WORD)],
                    body: vec![Instruction {
                        value: 7,
                        kind: WORD,
                        op: Operation::Call {
                            symbol: "ext".to_owned(),
                            arguments: vec![6],
                            inputs: vec![WORD],
                            kind: WORD,
                        },
                    }],
                    exit: Terminator::Jump(Target {
                        block: 2,
                        arguments: vec![7],
                    }),
                },
                Block {
                    id: 2,
                    parameters: vec![(8, WORD)],
                    body: vec![
                        Instruction {
                            value: 9,
                            kind: Type::Pair,
                            op: Operation::Pair { low: 8, high: 0 },
                        },
                        Instruction {
                            value: 10,
                            kind: WORD,
                            op: Operation::Low {
                                pair: 9,
                                kind: WORD,
                            },
                        },
                    ],
                    exit: Terminator::Return(Some(10)),
                },
            ],
        }],
    }
}

#[test]
fn valid() {
    let module = sample();
    assert!(verify(&module).is_ok());
}

#[test]
fn dominance() {
    let mut module = sample();
    module.functions[0].blocks[2].exit = Terminator::Return(Some(7));
    assert!(verify(&module).is_err());

    let mut ordered = sample();
    ordered.functions[0].blocks[0].body.swap(0, 2);
    assert!(verify(&ordered).is_err());
}

#[test]
fn arity() {
    let mut count = sample();
    count.functions[0].blocks[1].exit = Terminator::Jump(Target {
        block: 2,
        arguments: Vec::new(),
    });
    assert!(verify(&count).is_err());

    let mut mismatch = sample();
    mismatch.functions[0].blocks[1].exit = Terminator::Jump(Target {
        block: 2,
        arguments: vec![2],
    });
    assert!(verify(&mismatch).is_err());
}

#[test]
fn duplicates() {
    let mut syms = sample();
    syms.externals[0].symbol = "main".to_owned();
    assert!(verify(&syms).is_err());

    let mut vals = sample();
    vals.functions[0].blocks[0].body[1].value = 0;
    assert!(verify(&vals).is_err());

    let mut cases = sample();
    cases.functions[0].blocks[0].exit = Terminator::Switch {
        value: 0,
        cases: vec![
            (
                1,
                Target {
                    block: 1,
                    arguments: vec![0],
                },
            ),
            (
                1,
                Target {
                    block: 1,
                    arguments: vec![1],
                },
            ),
        ],
        default: Target {
            block: 2,
            arguments: vec![0],
        },
    };
    assert!(verify(&cases).is_err());
}

#[test]
fn asynchronous() {
    let mut syncyield = sample();
    syncyield.functions[0].blocks[0].body.push(Instruction {
        value: 11,
        kind: Type::Void,
        op: Operation::Yield,
    });
    assert!(verify(&syncyield).is_err());

    let mut badcall = sample();
    badcall.functions.push(Function {
        symbol: "task".to_owned(),
        linkage: Linkage::Internal,
        asynchronous: true,
        cleanup: None,
        inputs: Vec::new(),
        output: WORD,
        values: 1,
        blocks: vec![Block {
            id: 0,
            parameters: Vec::new(),
            body: vec![Instruction {
                value: 0,
                kind: WORD,
                op: Operation::Integer {
                    value: 1,
                    kind: WORD,
                },
            }],
            exit: Terminator::Return(Some(0)),
        }],
    });
    badcall.functions[0].blocks[0].body.push(Instruction {
        value: 12,
        kind: WORD,
        op: Operation::Call {
            symbol: "task".to_owned(),
            arguments: Vec::new(),
            inputs: Vec::new(),
            kind: WORD,
        },
    });
    assert!(verify(&badcall).is_err());
}
