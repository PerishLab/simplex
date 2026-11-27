use crate::{Platform, compile};
use simplex_ir::{
    Atomic, Binary, Block, Cast, Function, Instruction, Linkage, Module, Operation, Order, Target,
    Terminator, Type, Unary,
};
use simplex_object::Mode;

const fn word() -> Type {
    Type::Integer {
        bits: 64,
        signed: true,
    }
}

const fn flag() -> Type {
    Type::Integer {
        bits: 1,
        signed: false,
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

#[test]
fn spacious() {
    let real = Type::Float { bits: 64 };
    let edge = |b: u32, a: Vec<u32>| Target {
        block: b,
        arguments: a,
    };
    let b0 = block(
        0,
        Vec::new(),
        vec![
            step(
                2,
                Type::Pointer,
                Operation::Slot {
                    size: 8192,
                    align: 16,
                },
            ),
            step(
                3,
                Type::Pointer,
                Operation::Offset {
                    base: 2,
                    bytes: 6000,
                },
            ),
            step(
                4,
                Type::Void,
                Operation::Store {
                    address: 3,
                    value: 0,
                    kind: word(),
                    volatile: false,
                },
            ),
            step(
                5,
                word(),
                Operation::Acquire {
                    address: 3,
                    kind: word(),
                    order: Order::Acquire,
                },
            ),
            step(
                6,
                word(),
                Operation::Atomic {
                    op: Atomic::Add,
                    address: 3,
                    operand: 1,
                    kind: word(),
                    order: Order::Sequential,
                },
            ),
            step(
                7,
                flag(),
                Operation::Swap {
                    address: 3,
                    expected: 5,
                    desired: 6,
                    kind: word(),
                    order: Order::Sequential,
                },
            ),
        ],
        Terminator::Jump(edge(1, vec![0, 1])),
    );
    let b1 = block(
        1,
        vec![(8, word()), (9, word())],
        vec![step(
            10,
            flag(),
            Operation::Binary {
                op: Binary::Gt,
                left: 8,
                right: 9,
                kind: word(),
            },
        )],
        Terminator::Branch {
            condition: 10,
            truthy: edge(2, vec![8]),
            falsy: edge(1, vec![9, 8]),
        },
    );
    let b2 = block(
        2,
        vec![(11, word())],
        vec![
            step(
                12,
                real.clone(),
                Operation::Cast {
                    op: Cast::Promote,
                    value: 11,
                    from: word(),
                    kind: real.clone(),
                },
            ),
            step(
                13,
                real.clone(),
                Operation::Unary {
                    op: Unary::Neg,
                    value: 12,
                    kind: real.clone(),
                },
            ),
            step(
                14,
                word(),
                Operation::Cast {
                    op: Cast::Truncate,
                    value: 13,
                    from: real,
                    kind: word(),
                },
            ),
        ],
        Terminator::Switch {
            value: 14,
            cases: vec![(0, edge(3, Vec::new()))],
            default: edge(4, Vec::new()),
        },
    );
    let func = Function {
        symbol: String::from("sx$a$b$deep"),
        linkage: Linkage::Exported,
        asynchronous: false,
        cleanup: None,
        inputs: vec![(0, word()), (1, word())],
        output: word(),
        values: 32,
        blocks: vec![
            b0,
            b1,
            b2,
            block(3, Vec::new(), Vec::new(), Terminator::Trap { code: 1 }),
            block(4, Vec::new(), Vec::new(), Terminator::Return(Some(14))),
        ],
    };
    let out = compile(
        Platform::Apple,
        "deep",
        &Module {
            shared: true,
            functions: vec![func],
            ..Module::default()
        },
    )
    .expect("compile");
    assert_eq!(
        (out.mode, out.entry, out.code.len() % 4),
        (Mode::Shared, 0, 0)
    );
    let words: Vec<u32> = out
        .code
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    assert!(words.contains(&0xD420_0020));
}
