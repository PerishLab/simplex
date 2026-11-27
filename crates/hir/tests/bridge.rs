use simplex_hir::{bridge, lower, translate};
use simplex_ir::{Function, Linkage, Module, Operation, Target, Terminator, Type, prepare, verify};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

static SERIAL: AtomicUsize = AtomicUsize::new(0);

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let id = SERIAL.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("simplex-bridge-{label}-{}-{id}", process::id()));
        fs::create_dir_all(&dir).expect("mkdir");
        Self { dir }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn check(module: &Module) {
    verify(module).expect("ir verify");
    for item in &module.externals {
        assert!(!item.symbol.contains(".so"));
        if let Linkage::Dynamic(leaf) = &item.linkage {
            assert!(!leaf.starts_with("lib"));
            assert!(!leaf.contains('.'));
        }
    }
    for function in &module.functions {
        inspect(function);
    }
    let mut prepared = module.clone();
    prepare(&mut prepared).expect("ir prepare");
}

fn inspect(function: &Function) {
    assert!(!function.blocks.is_empty());
    let mut types = BTreeMap::new();
    for (id, kind) in &function.inputs {
        assert!(*id < function.values);
        assert_ne!(*kind, Type::Void);
        assert!(types.insert(*id, kind.clone()).is_none());
    }
    for (index, block) in function.blocks.iter().enumerate() {
        assert_eq!(block.id, index as u32);
        for (id, kind) in &block.parameters {
            assert!(*id < function.values);
            assert_ne!(*kind, Type::Void);
            assert!(types.insert(*id, kind.clone()).is_none());
        }
        for inst in &block.body {
            assert!(inst.value < function.values);
            assert!(types.insert(inst.value, inst.kind.clone()).is_none());
        }
    }
    for block in &function.blocks {
        for inst in &block.body {
            operands(&inst.op, &inst.kind, &types);
        }
        edges(&block.exit, function, &types);
    }
}

fn operands(op: &Operation, out: &Type, types: &BTreeMap<u32, Type>) {
    match op {
        Operation::Unary { value, kind, .. } => {
            assert_eq!(types.get(value), Some(kind));
            assert_eq!(out, kind);
        }
        Operation::Binary {
            left, right, kind, ..
        } => {
            assert_eq!(types.get(left), Some(kind));
            assert_eq!(types.get(right), Some(kind));
        }
        Operation::Cast {
            value, from, kind, ..
        } => {
            assert_eq!(types.get(value), Some(from));
            assert_eq!(out, kind);
        }
        Operation::Load { address, kind, .. } => {
            assert_eq!(types.get(address), Some(&Type::Pointer));
            assert_eq!(out, kind);
        }
        Operation::Store {
            address,
            value,
            kind,
            ..
        } => {
            assert_eq!(types.get(address), Some(&Type::Pointer));
            assert_eq!(types.get(value), Some(kind));
        }
        Operation::Call {
            arguments, inputs, ..
        }
        | Operation::Spawn {
            arguments, inputs, ..
        } => {
            assert_eq!(arguments.len(), inputs.len());
            for (arg, expected) in arguments.iter().zip(inputs) {
                assert_eq!(types.get(arg), Some(expected));
            }
        }
        _ => {}
    }
}

fn edges(exit: &Terminator, function: &Function, types: &BTreeMap<u32, Type>) {
    let boolean = Type::Integer {
        bits: 1,
        signed: false,
    };
    match exit {
        Terminator::Jump(target) => destination(target, function, types),
        Terminator::Branch {
            condition,
            truthy,
            falsy,
        } => {
            assert_eq!(types.get(condition), Some(&boolean));
            destination(truthy, function, types);
            destination(falsy, function, types);
        }
        Terminator::Switch {
            value,
            cases,
            default,
        } => {
            assert!(types.contains_key(value));
            destination(default, function, types);
            for (_, target) in cases {
                destination(target, function, types);
            }
        }
        Terminator::Return(Some(val)) => assert_eq!(types.get(val), Some(&function.output)),
        Terminator::Return(None) => assert_eq!(function.output, Type::Void),
        Terminator::Trap { .. } => {}
    }
}

fn destination(target: &Target, function: &Function, types: &BTreeMap<u32, Type>) {
    let block = function.blocks.get(target.block as usize).expect("target");
    assert_eq!(target.arguments.len(), block.parameters.len());
    for (arg, (_, expected)) in target.arguments.iter().zip(&block.parameters) {
        assert_eq!(types.get(arg), Some(expected));
    }
}

#[test]
fn golden() {
    let root = Path::new("../../cases");
    let mut total = 0usize;
    for domain in [
        "async", "borrow", "comptime", "control", "dylib", "lexical", "meta", "model", "module",
    ] {
        let dir = root.join(domain).join("pass");
        let mut files: Vec<_> = fs::read_dir(&dir)
            .expect("pass dir")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sx"))
            .collect();
        files.sort();
        for path in files {
            if path.file_stem().and_then(|s| s.to_str()) == Some("embed") {
                continue;
            }
            let shared = domain == "dylib" && path.file_stem().unwrap() != "plugin";
            let module = translate("app", &path, shared).expect("translate specimen");
            assert_eq!(module.shared, shared);
            check(&module);
            total += 1;
        }
    }
    assert_eq!(total, 37);
}

#[test]
fn embedded() {
    let scratch = Scratch::new("embed");
    fs::create_dir_all(scratch.dir.join("rules")).expect("mkdir");
    fs::write(scratch.dir.join("rules/default.toml"), b"abc").expect("asset");
    let source = fs::read_to_string("../../cases/meta/pass/embed.sx").expect("read embed.sx");
    let entry = scratch.dir.join("embed.sx");
    fs::write(&entry, source).expect("write embed.sx");
    let module = translate("app", &entry, false).expect("translate embed");
    assert!(module.assets.iter().any(|asset| asset.bytes == b"abc"));
    check(&module);
}

#[test]
fn linkage() {
    let plugin = Path::new("../../cases/dylib/pass/plugin.sx");
    let module = translate("app", plugin, false).expect("translate plugin");
    check(&module);
    let dynamic = module
        .externals
        .iter()
        .find(|item| item.symbol == "sx$app$codec$transcode")
        .expect("transcode import");
    assert_eq!(dynamic.linkage, Linkage::Dynamic(String::from("codec")));
    assert_eq!(dynamic.inputs[0], Type::Pair);

    let foreign = Path::new("../../cases/module/pass/ffi.sx");
    let linked = translate("app", foreign, false).expect("translate ffi");
    check(&linked);
    let system = linked
        .externals
        .iter()
        .find(|item| item.symbol == "epoll_wait")
        .expect("epoll_wait import");
    assert_eq!(system.linkage, Linkage::System);
    assert_eq!(
        system.output,
        Type::Integer {
            bits: 32,
            signed: true,
        }
    );
}

#[test]
fn aggregates() {
    let scratch = Scratch::new("aggregates");
    let entry = scratch.dir.join("shapes.sx");
    let raw = concat!(
        "Wide := struct {\n    alpha: i64\n    beta: i64\n    gamma: i64\n}\n\n",
        "State := enum {\n    idle\n    full(Wide)\n}\n\n",
        "make := (seed: i64) -> State {\n",
        "    State.full(Wide { alpha: seed, beta: seed + 1, gamma: seed + 2 })\n",
        "}\n\n",
        "pub main := () -> i64 {\n",
        "    var items: List<i64> = [10, 20, 30]\n",
        "    view := &items[0..2]\n",
        "    switch make(view[1] + items[0] - items[0]) {\n",
        "        State.idle => 0\n",
        "        State.full(wide) => wide.alpha + wide.alpha + wide.beta + wide.gamma + view.len().as<i64>()\n",
        "    }\n",
        "}\n",
    );
    let formatted =
        simplex::render(&simplex::parser::parse(&simplex::tokenize(raw).unwrap()).unwrap());
    fs::write(&entry, formatted).expect("write");
    let unit = lower("app", &entry, false).expect("lower");
    let module = bridge(&unit);
    check(&module);
    assert!(
        module
            .externals
            .iter()
            .any(|ext| ext.symbol == "sx$rt$alloc" && ext.linkage == Linkage::System)
    );
    let maker = module
        .functions
        .iter()
        .find(|func| func.symbol == "sx$app$shapes$make")
        .expect("make");
    assert_eq!(
        maker.output,
        Type::Aggregate {
            size: 32,
            align: 8,
            packed: false,
        }
    );
}
