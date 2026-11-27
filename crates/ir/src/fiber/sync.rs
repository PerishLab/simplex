use super::batch::{GATHER, RACE};
use super::builder::{SIGNED, UNSIGNED};
use super::layout::RESULT;
use super::runtime::{DRIVE, DROP};
use crate::{Cast, Function, Instruction, Operation, Type};

pub fn rewrite(function: &mut Function) {
    let mut types: Vec<Option<Type>> = vec![None; (function.values as usize).max(1)];
    for (id, kind) in &function.inputs {
        note(&mut types, *id, kind.clone());
    }
    for blk in &function.blocks {
        for (id, kind) in &blk.parameters {
            note(&mut types, *id, kind.clone());
        }
        for inst in &blk.body {
            if inst.kind.concrete() {
                note(&mut types, inst.value, inst.kind.clone());
            }
        }
    }
    let mut expander = Expander {
        types,
        next: function.values,
        out: Vec::new(),
    };
    for blk in &mut function.blocks {
        let old = std::mem::take(&mut blk.body);
        expander.out.reserve(old.len());
        for inst in old {
            expander.expand(inst);
        }
        blk.body = std::mem::take(&mut expander.out);
    }
    function.values = expander.next;
}

fn note(types: &mut Vec<Option<Type>>, id: u32, kind: Type) {
    let idx = id as usize;
    if idx >= types.len() {
        types.resize(idx + 1, None);
    }
    types[idx] = Some(kind);
}

struct Expander {
    types: Vec<Option<Type>>,
    next: u32,
    out: Vec<Instruction>,
}

impl Expander {
    fn alloc(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn expand(&mut self, inst: Instruction) {
        match inst.op {
            Operation::Spawn {
                symbol,
                arguments,
                inputs,
                ..
            } => {
                self.out.push(Instruction {
                    value: inst.value,
                    kind: Type::Pointer,
                    op: Operation::Call {
                        symbol,
                        arguments,
                        inputs,
                        kind: Type::Pointer,
                    },
                });
            }
            Operation::Await { fiber, kind } => self.awaiting(inst.value, fiber, kind),
            Operation::All {
                fibers,
                count,
                element,
            } => self.gathering(inst.value, (fibers, count), &element),
            Operation::Race {
                fibers,
                count,
                kind,
            } => self.racing(inst.value, (fibers, count), kind),
            _ => self.out.push(inst),
        }
    }

    fn awaiting(&mut self, dst: u32, fiber: u32, kind: Type) {
        let first = self.alloc();
        self.out.push(Instruction {
            value: first,
            kind: Type::Void,
            op: Operation::Call {
                symbol: DRIVE.to_owned(),
                arguments: vec![fiber],
                inputs: vec![Type::Pointer],
                kind: Type::Void,
            },
        });
        if kind.concrete() {
            let rptr = self.alloc();
            self.out.push(Instruction {
                value: rptr,
                kind: Type::Pointer,
                op: Operation::Offset {
                    base: fiber,
                    bytes: RESULT,
                },
            });
            self.out.push(Instruction {
                value: dst,
                kind: kind.clone(),
                op: Operation::Load {
                    address: rptr,
                    kind,
                    volatile: false,
                },
            });
        }
        let third = self.alloc();
        self.out.push(Instruction {
            value: third,
            kind: Type::Void,
            op: Operation::Call {
                symbol: DROP.to_owned(),
                arguments: vec![fiber],
                inputs: vec![Type::Pointer],
                kind: Type::Void,
            },
        });
    }

    fn gathering(&mut self, dst: u32, batch: (u32, u32), element: &Type) {
        let cnt = self.coerce(batch.1);
        let bytes = self.alloc();
        self.out.push(Instruction {
            value: bytes,
            kind: UNSIGNED,
            op: Operation::Integer {
                value: i128::from(element.bytes()),
                kind: UNSIGNED,
            },
        });
        self.out.push(Instruction {
            value: dst,
            kind: Type::Pair,
            op: Operation::Call {
                symbol: GATHER.to_owned(),
                arguments: vec![batch.0, cnt, bytes],
                inputs: vec![Type::Pointer, UNSIGNED, UNSIGNED],
                kind: Type::Pair,
            },
        });
    }

    fn racing(&mut self, dst: u32, batch: (u32, u32), kind: Type) {
        let cnt = self.coerce(batch.1);
        let slot = self.alloc();
        let span = kind.bytes().max(8);
        let align = kind.alignment().max(8);
        self.out.push(Instruction {
            value: slot,
            kind: Type::Pointer,
            op: Operation::Slot { size: span, align },
        });
        let bytes = self.alloc();
        self.out.push(Instruction {
            value: bytes,
            kind: UNSIGNED,
            op: Operation::Integer {
                value: i128::from(kind.bytes()),
                kind: UNSIGNED,
            },
        });
        let call = self.alloc();
        self.out.push(Instruction {
            value: call,
            kind: Type::Void,
            op: Operation::Call {
                symbol: RACE.to_owned(),
                arguments: vec![batch.0, cnt, slot, bytes],
                inputs: vec![Type::Pointer, UNSIGNED, Type::Pointer, UNSIGNED],
                kind: Type::Void,
            },
        });
        if kind.concrete() {
            self.out.push(Instruction {
                value: dst,
                kind: kind.clone(),
                op: Operation::Load {
                    address: slot,
                    kind,
                    volatile: false,
                },
            });
        }
    }

    fn coerce(&mut self, id: u32) -> u32 {
        if matches!(
            self.types.get(id as usize).and_then(Option::as_ref),
            Some(Type::Integer {
                bits: 64,
                signed: true
            })
        ) {
            let casted = self.alloc();
            self.out.push(Instruction {
                value: casted,
                kind: UNSIGNED,
                op: Operation::Cast {
                    op: Cast::Bitcast,
                    value: id,
                    from: SIGNED,
                    kind: UNSIGNED,
                },
            });
            return casted;
        }
        id
    }
}
