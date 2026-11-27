#[allow(dead_code)]
mod support;

use self::support::builder::{BYTE, Maker, SIGNED, UNSIGNED, emitter};
use self::support::engine::Machine;
use self::support::{Value, pure};
use simplex_ir::{Binary, External, Function, Module, Operation, Type, prepare};

fn wrap(externals: Vec<External>, functions: Vec<Function>) -> Module {
    Module {
        shared: false,
        entry: Some("main".to_owned()),
        assets: Vec::new(),
        tables: Vec::new(),
        externals,
        functions,
    }
}

#[test]
fn noop() {
    let mut m = Maker::new("main", false, SIGNED);
    let b0 = m.block();
    m.select(b0);
    let val = m.unsigned(42);
    let code = m.signed(val);
    m.ret(Some(code));
    let mut module = wrap(Vec::new(), vec![m.finish()]);
    prepare(&mut module).expect("prepare sync");
    assert!(module.externals.is_empty());
    assert_eq!(module.entry.as_deref(), Some("main"));
}

#[test]
fn sequential() {
    let packet = Type::Aggregate {
        size: 16,
        align: 8,
        packed: false,
    };
    let mut build = Maker::new("assemble", true, packet.clone());
    let code = build.input(UNSIGNED);
    let size = build.input(UNSIGNED);
    let b0 = build.block();
    build.select(b0);
    let slot = build.slot(16);
    build.store(slot, code, UNSIGNED);
    build.pause();
    let second = build.offset(slot, 8);
    build.store(second, size, UNSIGNED);
    let agg = build.load(slot, packet.clone());
    build.ret(Some(agg));

    let mut bump = Maker::new("bump", true, UNSIGNED);
    let ptr = bump.input(Type::Pointer);
    let delta = bump.input(UNSIGNED);
    let bb = bump.block();
    bump.select(bb);
    let prev = bump.load(ptr, UNSIGNED);
    bump.pause();
    let next = bump.binary(Binary::Add, prev, delta, UNSIGNED);
    bump.store(ptr, next, UNSIGNED);
    bump.ret(Some(next));

    let mut m = Maker::new("main", true, SIGNED);
    let mb = m.block();
    m.select(mb);
    let chan = m.slot(8);
    let init = m.unsigned(4);
    m.store(chan, init, UNSIGNED);
    let c21 = m.unsigned(21);
    let s30 = m.unsigned(30);
    let f1 = m.spawn(
        "assemble",
        vec![c21, s30],
        vec![UNSIGNED, UNSIGNED],
        packet.clone(),
    );
    let pkt = m.push(
        packet.clone(),
        Operation::Await {
            fiber: f1,
            kind: packet.clone(),
        },
    );
    let hold = m.slot(16);
    m.store(hold, pkt, packet);
    let pc = m.load(hold, UNSIGNED);
    let psptr = m.offset(hold, 8);
    let ps = m.load(psptr, UNSIGNED);
    let f2 = m.spawn(
        "bump",
        vec![chan, pc],
        vec![Type::Pointer, UNSIGNED],
        UNSIGNED,
    );
    let sent1 = m.push(
        UNSIGNED,
        Operation::Await {
            fiber: f2,
            kind: UNSIGNED,
        },
    );
    let cur = m.load(chan, UNSIGNED);
    let total = m.binary(Binary::Add, sent1, cur, UNSIGNED);
    let res = m.binary(Binary::Add, total, ps, UNSIGNED);
    let out = m.signed(res);
    m.ret(Some(out));

    let mut module = wrap(Vec::new(), vec![build.finish(), bump.finish(), m.finish()]);
    prepare(&mut module).expect("prepare sequential");
    assert!(pure(&module));
    let mut vm = Machine::new(&module);
    assert_eq!(vm.invoke("sx$entry$main", Vec::new()), Value::Word(80));
    assert!(vm.blocks.is_empty());
}

#[test]
fn unstarted() {
    let mut slow = Maker::new("slow", true, UNSIGNED);
    slow.cleanup("clean");
    let tag = slow.input(BYTE);
    let sb = slow.block();
    slow.select(sb);
    let env = slow.slot(16);
    let p = slow.offset(env, 8);
    slow.store(p, tag, BYTE);
    let one = slow.unsigned(1);
    slow.store(env, one, UNSIGNED);
    slow.pause();
    slow.pause();
    let v = slow.unsigned(9);
    slow.ret(Some(v));

    let mut outer = Maker::new("outer", true, UNSIGNED);
    let otag = outer.input(BYTE);
    let ob = outer.block();
    outer.select(ob);
    let child = outer.spawn("slow", vec![otag], vec![BYTE], UNSIGNED);
    let awaited = outer.push(
        UNSIGNED,
        Operation::Await {
            fiber: child,
            kind: UNSIGNED,
        },
    );
    outer.ret(Some(awaited));

    let mut clean = Maker::new("clean", false, Type::Void);
    let cenv = clean.input(Type::Pointer);
    let c0 = clean.block();
    let c1 = clean.block();
    let c2 = clean.block();
    clean.select(c0);
    let mask = clean.load(cenv, UNSIGNED);
    let z = clean.unsigned(0);
    let active = clean.binary(Binary::Ne, mask, z, UNSIGNED);
    clean.branch(active, (c1, Vec::new()), (c2, Vec::new()));
    clean.select(c1);
    let tp = clean.offset(cenv, 8);
    let ch = clean.load(tp, BYTE);
    clean.mark(ch);
    clean.jump(c2, Vec::new());
    clean.select(c2);
    clean.ret(None);

    let mut fast = Maker::new("fast", true, UNSIGNED);
    let fw = fast.input(BYTE);
    let fv = fast.input(UNSIGNED);
    let fb = fast.block();
    fast.select(fb);
    fast.mark(fw);
    fast.ret(Some(fv));

    let mut m = Maker::new("main", false, SIGNED);
    let mb = m.block();
    m.select(mb);
    let b9 = m.byte(b'9');
    let bw = m.byte(b'W');
    let w77 = m.unsigned(77);
    let b1 = m.byte(b'1');
    let f0 = m.spawn("outer", vec![b9], vec![BYTE], UNSIGNED);
    let f1 = m.spawn("fast", vec![bw, w77], vec![BYTE, UNSIGNED], UNSIGNED);
    let f2 = m.spawn("slow", vec![b1], vec![BYTE], UNSIGNED);
    let arr = m.slot(24);
    m.store(arr, f0, Type::Pointer);
    let s1 = m.offset(arr, 8);
    m.store(s1, f1, Type::Pointer);
    let s2 = m.offset(arr, 16);
    m.store(s2, f2, Type::Pointer);
    let cnt = m.unsigned(3);
    let won = m.push(
        UNSIGNED,
        Operation::Race {
            fibers: arr,
            count: cnt,
            kind: UNSIGNED,
        },
    );
    let out = m.signed(won);
    m.ret(Some(out));

    let funcs = vec![
        slow.finish(),
        outer.finish(),
        clean.finish(),
        fast.finish(),
        m.finish(),
    ];
    let mut module = wrap(vec![emitter()], funcs);
    prepare(&mut module).expect("prepare nested and unstarted");
    assert!(pure(&module));
    let mut vm = Machine::new(&module);
    assert_eq!(vm.invoke("main", Vec::new()), Value::Word(77));
    assert_eq!(vm.stdout, b"W9");
    assert!(vm.blocks.is_empty());
}

#[test]
fn recycling() {
    let mut slow = Maker::new("slow", true, UNSIGNED);
    let sv = slow.input(UNSIGNED);
    let sb = slow.block();
    slow.select(sb);
    slow.pause();
    slow.pause();
    slow.ret(Some(sv));

    let mut fast = Maker::new("fast", true, UNSIGNED);
    let fv = fast.input(UNSIGNED);
    let fb = fast.block();
    fast.select(fb);
    fast.ret(Some(fv));

    let mut m = Maker::new("main", true, SIGNED);
    let entry = m.block();
    let head = m.block();
    let body = m.block();
    let done = m.block();
    let idx = m.parameter(head, UNSIGNED);
    let total = m.parameter(head, UNSIGNED);
    m.select(entry);
    let z = m.unsigned(0);
    m.jump(head, vec![z, z]);
    m.select(head);
    let limit = m.unsigned(16);
    let more = m.binary(Binary::Lt, idx, limit, UNSIGNED);
    m.branch(more, (body, Vec::new()), (done, Vec::new()));
    m.select(body);
    let one = m.unsigned(1);
    let plus = m.binary(Binary::Add, idx, one, UNSIGNED);
    let f0 = m.spawn("slow", vec![idx], vec![UNSIGNED], UNSIGNED);
    let f1 = m.spawn("fast", vec![plus], vec![UNSIGNED], UNSIGNED);
    let arr = m.slot(16);
    m.store(arr, f0, Type::Pointer);
    let s1 = m.offset(arr, 8);
    m.store(s1, f1, Type::Pointer);
    let cnt = m.unsigned(2);
    let won = m.push(
        UNSIGNED,
        Operation::Race {
            fibers: arr,
            count: cnt,
            kind: UNSIGNED,
        },
    );
    let nexttot = m.binary(Binary::Add, total, won, UNSIGNED);
    m.jump(head, vec![plus, nexttot]);
    m.select(done);
    let out = m.signed(total);
    m.ret(Some(out));

    let mut module = wrap(Vec::new(), vec![slow.finish(), fast.finish(), m.finish()]);
    prepare(&mut module).expect("prepare recycling");
    assert!(pure(&module));
    let mut vm = Machine::new(&module);
    assert_eq!(vm.invoke("sx$entry$main", Vec::new()), Value::Word(136));
    assert!(vm.blocks.is_empty());
}
