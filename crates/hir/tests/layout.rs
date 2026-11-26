use simplex_hir::{Catalog, Footprint, Mangle, Passing, Type, lower};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

static SERIAL: AtomicUsize = AtomicUsize::new(0);

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let id = SERIAL.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("simplex-hir-{label}-{}-{id}", process::id()));
        fs::create_dir_all(&dir).expect("mkdir");
        Self { dir }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn scalars() {
    let catalog = Catalog::default();
    assert_eq!(Type::Void.footprint(&catalog), Footprint::VOID);
    assert_eq!(Type::Bool.footprint(&catalog), Footprint::scalar(1));
    assert_eq!(Type::Signed(1).footprint(&catalog), Footprint::scalar(1));
    assert_eq!(Type::Unsigned(2).footprint(&catalog), Footprint::scalar(2));
    assert_eq!(Type::Signed(4).footprint(&catalog), Footprint::scalar(4));
    assert_eq!(Type::Float(4).footprint(&catalog), Footprint::scalar(4));
    assert_eq!(Type::Unsigned(8).footprint(&catalog), Footprint::scalar(8));
    assert_eq!(Type::Float(8).footprint(&catalog), Footprint::scalar(8));
    assert_eq!(
        Type::Pointer(Box::new(Type::Bool)).footprint(&catalog),
        Footprint::scalar(8)
    );
    assert_eq!(
        Type::Fiber(Box::new(Type::Unsigned(8))).footprint(&catalog),
        Footprint::scalar(8)
    );
    assert_eq!(
        Type::Slice(Box::new(Type::Unsigned(1))).footprint(&catalog),
        Footprint::pair()
    );
    assert_eq!(
        Type::Trait(String::from("Codec")).footprint(&catalog),
        Footprint::pair()
    );
    assert_eq!(
        Type::List(Box::new(Type::Signed(8))).footprint(&catalog),
        Footprint::pair()
    );
}

#[test]
fn aggregates() {
    let scratch = Scratch::new("layout");
    let path = scratch.dir.join("shapes.sx");
    let source = concat!(
        "Small := struct {\n    code: u32\n    flag: u8\n}\n\n",
        "Medium := struct {\n    first: u64\n    second: u32\n}\n\n",
        "Wide := struct {\n    tag: u8\n    count: u64\n    tail: u16\n}\n\n",
        "Choice := enum {\n    idle\n    single(u64)\n    triple(u64, u64, u64)\n}\n\n",
        "pub main := () -> i64 {\n",
        "    small := Small { code: 1, flag: 2 }\n",
        "    medium := Medium { first: 3, second: 4 }\n",
        "    wide := Wide { tag: 5, count: 6, tail: 7 }\n",
        "    var choice := Choice.single(8)\n",
        "    total := small.code.into<u64>() + medium.first + wide.count +\n",
        "        small.flag.into<u64>() + medium.second.into<u64>() + wide.tail.into<u64>()\n",
        "    switch choice {\n",
        "        Choice.idle => total.as<i64>()\n",
        "        Choice.single(v) => (total + v + v).as<i64>()\n",
        "        Choice.triple(a, b, c) => (a + a + b + b + c + c).as<i64>()\n",
        "    }\n",
        "}\n",
    );
    let formatted =
        simplex::render(&simplex::parser::parse(&simplex::tokenize(source).unwrap()).unwrap());
    fs::write(&path, formatted).expect("write");
    let unit = lower("app", &path, false).expect("lower shapes");

    let small = unit.catalog.records.get("Small").expect("Small");
    assert_eq!(small.footprint.size, 8);
    assert_eq!(small.footprint.align, 4);
    assert_eq!(small.footprint.passing, Passing::Register);

    let medium = unit.catalog.records.get("Medium").expect("Medium");
    assert_eq!(medium.footprint.size, 16);
    assert_eq!(medium.footprint.align, 8);
    assert_eq!(medium.footprint.passing, Passing::Pair);

    let wide = unit.catalog.records.get("Wide").expect("Wide");
    assert_eq!(wide.fields[0].offset, 0);
    assert_eq!(wide.fields[1].offset, 8);
    assert_eq!(wide.fields[2].offset, 16);
    assert_eq!(wide.footprint.size, 24);
    assert_eq!(wide.footprint.align, 8);
    assert_eq!(wide.footprint.passing, Passing::Indirect);

    let choice = unit.catalog.choices.get("Choice").expect("Choice");
    assert_eq!(choice.discriminant, 1);
    assert_eq!(choice.footprint.size, 32);
    assert_eq!(choice.footprint.passing, Passing::Indirect);
    assert_eq!(choice.arms[1].payloads[0].offset, 8);
}

#[test]
fn mangling() {
    let vtable = Mangle::table("app", "plugin", "Xor", "Codec");
    assert_eq!(vtable, "sx$vt$app$plugin$Xor$Codec");
    assert_eq!(
        Mangle::method(&vtable, "encode"),
        "sx$impl$app$plugin$Xor$Codec$encode"
    );
    assert_eq!(
        Mangle::symbol("app", "codec", "digest"),
        "sx$app$codec$digest"
    );
    assert_eq!(Mangle::asset("app", "embed", 0), "sx$ro$app$embed$0");
}
