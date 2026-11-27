#[allow(dead_code)]
mod support;

use self::support::builder::{BYTE, Maker, SIGNED, UNSIGNED, emitter};
use self::support::engine::Machine;
use self::support::{Value, pure};
use simplex_ir::{Binary, External, Function, Linkage, Module, Operation, Type, prepare};

fn slow() -> Function {
    let mut m = Maker::new("slow", true, UNSIGNED);
    m.cleanup("clean");
    let first = m.input(BYTE);
    let second = m.input(BYTE);
    let start = m.input(BYTE);
    let entry = m.block();
    m.select(entry);
    let env = m.slot(16);
    let p1 = m.offset(env, 8);
    m.store(p1, first, BYTE);
    let b1 = m.unsigned(1);
    m.store(env, b1, UNSIGNED);
    let p2 = m.offset(env, 9);
    m.store(p2, second, BYTE);
    let b3 = m.unsigned(3);
    m.store(env, b3, UNSIGNED);
    m.mark(start);
    m.pause();
    m.pause();
    let bad = m.byte(b'X');
    m.mark(bad);
    let z = m.unsigned(0);
    m.store(env, z, UNSIGNED);
    let ans = m.unsigned(99);
    m.ret(Some(ans));
    m.finish()
}

fn clean() -> Function {
    let mut m = Maker::new("clean", false, Type::Void);
    let env = m.input(Type::Pointer);
    let b0 = m.block();
    let b1 = m.block();
    let b2 = m.block();
    let b3 = m.block();
    let b4 = m.block();
    m.select(b0);
    let mask = m.load(env, UNSIGNED);
    let two = m.unsigned(2);
    let zero = m.unsigned(0);
    let m2 = m.binary(Binary::And, mask, two, UNSIGNED);
    let has2 = m.binary(Binary::Ne, m2, zero, UNSIGNED);
    m.branch(has2, (b1, Vec::new()), (b2, Vec::new()));
    m.select(b1);
    let p2 = m.offset(env, 9);
    let g2 = m.load(p2, BYTE);
    m.mark(g2);
    m.jump(b2, Vec::new());
    m.select(b2);
    let one = m.unsigned(1);
    let m1 = m.binary(Binary::And, mask, one, UNSIGNED);
    let has1 = m.binary(Binary::Ne, m1, zero, UNSIGNED);
    m.branch(has1, (b3, Vec::new()), (b4, Vec::new()));
    m.select(b3);
    let p1 = m.offset(env, 8);
    let g1 = m.load(p1, BYTE);
    m.mark(g1);
    m.jump(b4, Vec::new());
    m.select(b4);
    m.ret(None);
    m.finish()
}

fn fast() -> Function {
    let mut m = Maker::new("fast", true, UNSIGNED);
    let banner = m.input(BYTE);
    let win = m.input(UNSIGNED);
    let b0 = m.block();
    m.select(b0);
    m.mark(banner);
    m.ret(Some(win));
    m.finish()
}

#[test]
fn gather() {
    let mut s = Maker::new("step", true, UNSIGNED);
    let a = s.input(BYTE);
    let b = s.input(BYTE);
    let v = s.input(UNSIGNED);
    let sb = s.block();
    s.select(sb);
    s.mark(a);
    s.pause();
    s.mark(b);
    s.ret(Some(v));

    let mut m = Maker::new("main", true, SIGNED);
    let mb = m.block();
    m.select(mb);
    let ca = m.byte(b'A');
    let cc = m.byte(b'C');
    let v1 = m.unsigned(11);
    let cb = m.byte(b'B');
    let cd = m.byte(b'D');
    let v2 = m.unsigned(22);
    let f1 = m.spawn(
        "step",
        vec![ca, cc, v1],
        vec![BYTE, BYTE, UNSIGNED],
        UNSIGNED,
    );
    let f2 = m.spawn(
        "step",
        vec![cb, cd, v2],
        vec![BYTE, BYTE, UNSIGNED],
        UNSIGNED,
    );
    let arr = m.slot(16);
    m.store(arr, f1, Type::Pointer);
    let second = m.offset(arr, 8);
    m.store(second, f2, Type::Pointer);
    let cnt = m.unsigned(2);
    let pair = m.push(
        Type::Pair,
        Operation::All {
            fibers: arr,
            count: cnt,
            element: UNSIGNED,
        },
    );
    let ptr = m.push(
        Type::Pointer,
        Operation::Low {
            pair,
            kind: Type::Pointer,
        },
    );
    let r1 = m.load(ptr, UNSIGNED);
    let p2 = m.offset(ptr, 8);
    let r2 = m.load(p2, UNSIGNED);
    let sum = m.binary(Binary::Add, r1, r2, UNSIGNED);
    let sz = m.unsigned(16);
    let _ = m.push(
        Type::Void,
        Operation::Call {
            symbol: "sx$rt$free".to_owned(),
            arguments: vec![ptr, sz],
            inputs: vec![Type::Pointer, UNSIGNED],
            kind: Type::Void,
        },
    );
    let code = m.signed(sum);
    m.ret(Some(code));

    let mut module = Module {
        shared: false,
        entry: Some("main".to_owned()),
        assets: Vec::new(),
        tables: Vec::new(),
        externals: vec![
            emitter(),
            External {
                symbol: "sx$rt$free".to_owned(),
                linkage: Linkage::System,
                inputs: vec![Type::Pointer, UNSIGNED],
                output: Type::Void,
            },
        ],
        functions: vec![s.finish(), m.finish()],
    };
    prepare(&mut module).expect("prepare gather");
    assert!(pure(&module));
    assert_eq!(module.entry.as_deref(), Some("sx$entry$main"));
    let mut vm = Machine::new(&module);
    let exit = vm.invoke("sx$entry$main", Vec::new());
    assert_eq!(exit, Value::Word(33));
    assert_eq!(vm.stdout, b"ABCD");
    assert!(vm.blocks.is_empty());
}

#[test]
fn cancellation() {
    let mut m = Maker::new("main", true, SIGNED);
    let mb = m.block();
    m.select(mb);
    let g1 = m.byte(b'1');
    let g2 = m.byte(b'2');
    let sa = m.byte(b'a');
    let g3 = m.byte(b'3');
    let g4 = m.byte(b'4');
    let sb = m.byte(b'b');
    let bw = m.byte(b'W');
    let wv = m.unsigned(77);
    let f1 = m.spawn("slow", vec![g1, g2, sa], vec![BYTE, BYTE, BYTE], UNSIGNED);
    let f2 = m.spawn("slow", vec![g3, g4, sb], vec![BYTE, BYTE, BYTE], UNSIGNED);
    let f3 = m.spawn("fast", vec![bw, wv], vec![BYTE, UNSIGNED], UNSIGNED);
    let arr = m.slot(24);
    m.store(arr, f1, Type::Pointer);
    let s1 = m.offset(arr, 8);
    m.store(s1, f2, Type::Pointer);
    let s2 = m.offset(arr, 16);
    m.store(s2, f3, Type::Pointer);
    let cnt = m.unsigned(3);
    let won = m.push(
        UNSIGNED,
        Operation::Race {
            fibers: arr,
            count: cnt,
            kind: UNSIGNED,
        },
    );
    let code = m.signed(won);
    m.ret(Some(code));

    let mut module = Module {
        shared: false,
        entry: Some("main".to_owned()),
        assets: Vec::new(),
        tables: Vec::new(),
        externals: vec![emitter()],
        functions: vec![slow(), clean(), fast(), m.finish()],
    };
    prepare(&mut module).expect("prepare cancellation");
    assert!(pure(&module));
    let mut vm = Machine::new(&module);
    let exit = vm.invoke("sx$entry$main", Vec::new());
    assert_eq!(exit, Value::Word(77));
    assert_eq!(vm.stdout, b"abW4321");
    assert!(vm.blocks.is_empty());
}
