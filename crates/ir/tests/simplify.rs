use simplex_ir::{
    Binary, Block, Cast, Function, Instruction, Linkage, Module, Operation, Target, Terminator,
    Type, Unary, simplify, verify,
};

const WORD: Type = Type::Integer {
    bits: 64,
    signed: true,
};
const BYTE: Type = Type::Integer {
    bits: 8,
    signed: false,
};
const FLAG: Type = Type::Integer {
    bits: 1,
    signed: false,
};

#[test]
fn folding() {
    let mut module = Module {
        shared: false,
        entry: Some("main".to_owned()),
        assets: Vec::new(),
        tables: Vec::new(),
        externals: Vec::new(),
        functions: vec![Function {
            symbol: "main".to_owned(),
            linkage: Linkage::Exported,
            asynchronous: false,
            cleanup: None,
            inputs: Vec::new(),
            output: WORD,
            values: 10,
            blocks: vec![
                Block {
                    id: 0,
                    parameters: Vec::new(),
                    body: vec![
                        Instruction {
                            value: 0,
                            kind: WORD,
                            op: Operation::Integer {
                                value: 6,
                                kind: WORD,
                            },
                        },
                        Instruction {
                            value: 1,
                            kind: WORD,
                            op: Operation::Integer {
                                value: 7,
                                kind: WORD,
                            },
                        },
                        Instruction {
                            value: 2,
                            kind: WORD,
                            op: Operation::Binary {
                                op: Binary::Mul,
                                left: 0,
                                right: 1,
                                kind: WORD,
                            },
                        },
                        Instruction {
                            value: 3,
                            kind: FLAG,
                            op: Operation::Binary {
                                op: Binary::Gt,
                                left: 2,
                                right: 1,
                                kind: WORD,
                            },
                        },
                    ],
                    exit: Terminator::Branch {
                        condition: 3,
                        truthy: Target {
                            block: 1,
                            arguments: vec![2],
                        },
                        falsy: Target {
                            block: 2,
                            arguments: Vec::new(),
                        },
                    },
                },
                Block {
                    id: 1,
                    parameters: vec![(4, WORD)],
                    body: vec![Instruction {
                        value: 5,
                        kind: WORD,
                        op: Operation::Unary {
                            op: Unary::Neg,
                            value: 4,
                            kind: WORD,
                        },
                    }],
                    exit: Terminator::Return(Some(5)),
                },
                Block {
                    id: 2,
                    parameters: Vec::new(),
                    body: vec![Instruction {
                        value: 6,
                        kind: WORD,
                        op: Operation::Integer {
                            value: 99,
                            kind: WORD,
                        },
                    }],
                    exit: Terminator::Return(Some(6)),
                },
            ],
        }],
    };
    assert!(verify(&module).is_ok());
    simplify(&mut module);
    assert!(verify(&module).is_ok());
    assert_eq!(module.functions[0].blocks.len(), 2);
    assert_eq!(
        module.functions[0].blocks[0].exit,
        Terminator::Jump(Target {
            block: 1,
            arguments: vec![2]
        })
    );
    assert_eq!(
        module.functions[0].blocks[0].body,
        vec![Instruction {
            value: 2,
            kind: WORD,
            op: Operation::Integer {
                value: 42,
                kind: WORD
            }
        }]
    );
}

#[test]
fn propagation() {
    let mut module = Module {
        shared: false,
        entry: Some("main".to_owned()),
        assets: Vec::new(),
        tables: Vec::new(),
        externals: Vec::new(),
        functions: vec![Function {
            symbol: "main".to_owned(),
            linkage: Linkage::Exported,
            asynchronous: false,
            cleanup: None,
            inputs: vec![(0, Type::Pointer), (1, BYTE)],
            output: BYTE,
            values: 8,
            blocks: vec![Block {
                id: 0,
                parameters: Vec::new(),
                body: vec![
                    Instruction {
                        value: 2,
                        kind: Type::Pointer,
                        op: Operation::Offset { base: 0, bytes: 0 },
                    },
                    Instruction {
                        value: 3,
                        kind: BYTE,
                        op: Operation::Cast {
                            op: Cast::Widen,
                            value: 1,
                            from: BYTE,
                            kind: BYTE,
                        },
                    },
                    Instruction {
                        value: 4,
                        kind: Type::Pair,
                        op: Operation::Pair { low: 2, high: 3 },
                    },
                    Instruction {
                        value: 5,
                        kind: Type::Pointer,
                        op: Operation::Low {
                            pair: 4,
                            kind: Type::Pointer,
                        },
                    },
                    Instruction {
                        value: 6,
                        kind: BYTE,
                        op: Operation::High {
                            pair: 4,
                            kind: BYTE,
                        },
                    },
                    Instruction {
                        value: 7,
                        kind: Type::Void,
                        op: Operation::Store {
                            address: 5,
                            value: 6,
                            kind: BYTE,
                            volatile: false,
                        },
                    },
                ],
                exit: Terminator::Return(Some(6)),
            }],
        }],
    };
    assert!(verify(&module).is_ok());
    simplify(&mut module);
    assert!(verify(&module).is_ok());
    assert_eq!(
        module.functions[0].blocks[0].body,
        vec![Instruction {
            value: 7,
            kind: Type::Void,
            op: Operation::Store {
                address: 0,
                value: 1,
                kind: BYTE,
                volatile: false
            },
        }]
    );
    assert_eq!(
        module.functions[0].blocks[0].exit,
        Terminator::Return(Some(1))
    );
}
