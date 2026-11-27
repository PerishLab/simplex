use super::{Plan, Tracked};
use crate::bridge::types::{BOOL, WIDE, convert};
use crate::tree::Unit;
use simplex_ir::{Binary, Block, Function, Instruction, Linkage, Operation, Target, Terminator};

pub fn synthesize(symbol: &str, spec: &Plan, unit: &Unit) -> Function {
    let mut values = 1u32;
    let mut blocks = Vec::new();
    let total = spec.items.len();
    for (step, item) in spec.items.iter().rev().enumerate() {
        let check = (step * 2) as u32;
        let action = check + 1;
        let next = ((step + 1) * 2) as u32;
        blocks.push(guard(check, (action, next), item, &mut values));
        blocks.push(dropper(action, next, (item, unit), &mut values));
    }
    blocks.push(Block {
        id: (total * 2) as u32,
        parameters: Vec::new(),
        body: Vec::new(),
        exit: Terminator::Return(None),
    });
    Function {
        symbol: format!("sx$clean${symbol}"),
        linkage: Linkage::Internal,
        asynchronous: false,
        cleanup: None,
        inputs: vec![(0, simplex_ir::Type::Pointer)],
        output: simplex_ir::Type::Void,
        values,
        blocks,
    }
}

fn guard(id: u32, edges: (u32, u32), item: &Tracked, values: &mut u32) -> Block {
    let mask = take(values);
    let flag = take(values);
    let masked = take(values);
    let zero = take(values);
    let active = take(values);
    let body = vec![
        Instruction {
            value: mask,
            kind: WIDE,
            op: Operation::Load {
                address: 0,
                kind: WIDE,
                volatile: false,
            },
        },
        Instruction {
            value: flag,
            kind: WIDE,
            op: Operation::Integer {
                value: 1i128 << item.bit,
                kind: WIDE,
            },
        },
        Instruction {
            value: masked,
            kind: WIDE,
            op: Operation::Binary {
                op: Binary::And,
                left: mask,
                right: flag,
                kind: WIDE,
            },
        },
        Instruction {
            value: zero,
            kind: WIDE,
            op: Operation::Integer {
                value: 0,
                kind: WIDE,
            },
        },
        Instruction {
            value: active,
            kind: BOOL,
            op: Operation::Binary {
                op: Binary::Ne,
                left: masked,
                right: zero,
                kind: WIDE,
            },
        },
    ];
    Block {
        id,
        parameters: Vec::new(),
        body,
        exit: Terminator::Branch {
            condition: active,
            truthy: Target {
                block: edges.0,
                arguments: Vec::new(),
            },
            falsy: Target {
                block: edges.1,
                arguments: Vec::new(),
            },
        },
    }
}

fn dropper(id: u32, next: u32, target: (&Tracked, &Unit), values: &mut u32) -> Block {
    let (item, unit) = target;
    let mask = take(values);
    let clear = take(values);
    let updated = take(values);
    let store = take(values);
    let address = take(values);
    let mut body = vec![
        Instruction {
            value: mask,
            kind: WIDE,
            op: Operation::Load {
                address: 0,
                kind: WIDE,
                volatile: false,
            },
        },
        Instruction {
            value: clear,
            kind: WIDE,
            op: Operation::Integer {
                value: (!(1u64 << item.bit) as i64) as i128,
                kind: WIDE,
            },
        },
        Instruction {
            value: updated,
            kind: WIDE,
            op: Operation::Binary {
                op: Binary::And,
                left: mask,
                right: clear,
                kind: WIDE,
            },
        },
        Instruction {
            value: store,
            kind: simplex_ir::Type::Void,
            op: Operation::Store {
                address: 0,
                value: updated,
                kind: WIDE,
                volatile: false,
            },
        },
        Instruction {
            value: address,
            kind: simplex_ir::Type::Pointer,
            op: Operation::Offset {
                base: 0,
                bytes: i64::from(item.offset),
            },
        },
    ];
    let (argument, fallback) = match item.borrowed {
        true => (address, simplex_ir::Type::Pointer),
        false => {
            let lowered = convert(&item.kind, &unit.catalog);
            let loaded = take(values);
            body.push(Instruction {
                value: loaded,
                kind: lowered.clone(),
                op: Operation::Load {
                    address,
                    kind: lowered.clone(),
                    volatile: false,
                },
            });
            (loaded, lowered)
        }
    };
    let (callee, input, out) = signature(&item.symbol, fallback, unit);
    let call = take(values);
    body.push(Instruction {
        value: call,
        kind: out.clone(),
        op: Operation::Call {
            symbol: callee,
            arguments: vec![argument],
            inputs: vec![input],
            kind: out,
        },
    });
    Block {
        id,
        parameters: Vec::new(),
        body,
        exit: Terminator::Jump(Target {
            block: next,
            arguments: Vec::new(),
        }),
    }
}

fn signature(
    symbol: &str,
    fallback: simplex_ir::Type,
    unit: &Unit,
) -> (String, simplex_ir::Type, simplex_ir::Type) {
    if let Some(routine) = unit.routines.iter().find(|item| item.symbol == symbol) {
        let input = routine
            .inputs
            .first()
            .map(|(_, kind)| convert(kind, &unit.catalog))
            .unwrap_or(fallback);
        let out = convert(&routine.output, &unit.catalog);
        return (routine.symbol.clone(), input, out);
    }
    if let Some(imported) = unit
        .imports
        .iter()
        .find(|item| item.symbol == symbol || item.local == symbol)
    {
        let input = imported
            .inputs
            .first()
            .map(|kind| convert(kind, &unit.catalog))
            .unwrap_or(fallback);
        let out = convert(&imported.output, &unit.catalog);
        return (imported.symbol.clone(), input, out);
    }
    (symbol.to_owned(), fallback, simplex_ir::Type::Void)
}

fn take(values: &mut u32) -> u32 {
    let id = *values;
    *values += 1;
    id
}
