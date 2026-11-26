use simplex_emit::Register;
use simplex_emit::encode::Condition;
use simplex_emit::machine::allocate::{Location, allocate};
use simplex_emit::machine::emit;
use simplex_emit::machine::linear::{
    Binary, Callee, Ending, Invocation, Parameter, Procedure, Step,
};

#[test]
fn spilling() {
    let mut proc = Procedure::new("pressure");
    let mut vals = Vec::new();
    for idx in 0..14 {
        let v = proc.value();
        proc.emit(Step::Constant {
            dst: v,
            value: idx + 1,
        });
        vals.push(v);
    }
    let mid = proc.value();
    proc.emit(Step::Invoke(Invocation {
        callee: Callee::Direct {
            symbol: "sx$app$main$leaf".to_owned(),
            external: false,
        },
        registers: vec![vals[0]],
        floats: Vec::new(),
        stack: Vec::new(),
        space: 0,
        first: Some(mid),
        second: None,
        media: None,
    }));
    let mut acc = mid;
    for v in vals {
        let next = proc.value();
        proc.emit(Step::Binary {
            kind: Binary::Add,
            dst: next,
            lhs: acc,
            rhs: v,
        });
        acc = next;
    }
    proc.emit(Step::Finish(Ending::Single(acc)));
    let frame = allocate(&proc);
    let spills = frame
        .locations
        .iter()
        .filter(|loc| matches!(loc, Location::Spill(_)))
        .count();
    assert!(spills >= 8);
    for loc in &frame.locations[0..14] {
        if let Location::Register(reg) = loc {
            assert!(Register::PRESERVED.contains(reg));
        }
    }
    let total = (frame.preserved.len() as i32 * 8) + frame.reserve;
    assert_eq!(total % 16, 0);
    let (bytes, relocs) = emit(&proc).expect("emit spilled procedure");
    assert!(!bytes.is_empty());
    assert_eq!(relocs.len(), 1);
}

#[test]
fn loops() {
    let mut proc = Procedure::new("counter");
    let limit = proc.value();
    proc.parameters.push(Parameter::Register {
        dst: limit,
        index: 0,
    });
    let step = proc.value();
    proc.emit(Step::Constant {
        dst: step,
        value: 1,
    });
    let zero = proc.value();
    proc.emit(Step::Constant {
        dst: zero,
        value: 0,
    });
    let slot = proc.reserve(8, 8);
    let addr = proc.value();
    proc.emit(Step::Slot {
        dst: addr,
        slot,
        delta: 0,
    });
    let head = proc.label();
    let done = proc.label();
    proc.emit(Step::Mark(head));
    let cmp = proc.value();
    proc.emit(Step::Binary {
        kind: Binary::Compare(Condition::Equal),
        dst: cmp,
        lhs: limit,
        rhs: zero,
    });
    proc.emit(Step::Branch {
        cond: cmp,
        truth: true,
        target: done,
    });
    proc.emit(Step::Jump(head));
    proc.emit(Step::Mark(done));
    let out = proc.value();
    proc.emit(Step::Binary {
        kind: Binary::Add,
        dst: out,
        lhs: addr,
        rhs: step,
    });
    proc.emit(Step::Finish(Ending::Single(out)));
    let frame = allocate(&proc);
    let total = (frame.preserved.len() as i32 * 8) + frame.reserve;
    assert_eq!(total % 16, 0);
    let (bytes, _) = emit(&proc).expect("emit loop");
    assert_eq!(bytes[bytes.len() - 1], 0xC3);
}
