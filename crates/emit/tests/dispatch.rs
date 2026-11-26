use simplex_elf::{Kind, Mode};
use simplex_emit::compile;
use simplex_hir::{
    Asset, Catalog, Expression, Field, Footprint, Layout, Mangle, Routine, Statement, Table, Type,
    Unit,
};

#[test]
fn dispatch() {
    let mut catalog = Catalog::default();
    catalog.records.insert(
        "Xor".to_owned(),
        Layout {
            name: "Xor".to_owned(),
            footprint: Footprint::scalar(1),
            fields: vec![Field {
                name: "key".to_owned(),
                offset: 0,
                kind: Type::Unsigned(1),
            }],
        },
    );
    let vtable = Mangle::table("app", "codec", "Xor", "Codec");
    let method = Mangle::method(&vtable, "encode");
    let run = Mangle::symbol("app", "codec", "run");
    let asset = Mangle::asset("app", "codec", 0);
    let xor = Type::Record("Xor".to_owned());
    let ptr = Type::Pointer(Box::new(xor.clone()));
    let traitref = Type::Trait("Codec".to_owned());
    let slice = Type::Slice(Box::new(Type::Unsigned(1)));
    let unit = Unit {
        namespace: "app".to_owned(),
        shared: true,
        libraries: Vec::new(),
        imports: Vec::new(),
        assets: vec![Asset {
            symbol: asset.clone(),
            bytes: vec![10, 20, 30, 40],
        }],
        tables: vec![Table {
            symbol: vtable.clone(),
            subject: "Xor".to_owned(),
            contract: "Codec".to_owned(),
            methods: vec![method.clone()],
        }],
        catalog,
        routines: vec![
            Routine {
                symbol: method.clone(),
                exported: true,
                asynchronous: false,
                inputs: vec![
                    ("self".to_owned(), ptr.clone()),
                    ("input".to_owned(), slice.clone()),
                ],
                output: Type::Unsigned(8),
                body: vec![Statement::Result(Expression::Length(Box::new(
                    Expression::Local("input".to_owned(), slice.clone()),
                )))],
            },
            Routine {
                symbol: run,
                exported: true,
                asynchronous: false,
                inputs: Vec::new(),
                output: Type::Unsigned(8),
                body: vec![
                    Statement::Bind {
                        name: "item".to_owned(),
                        mutable: false,
                        kind: xor.clone(),
                        value: Expression::Record {
                            name: "Xor".to_owned(),
                            fields: vec![("key".to_owned(), Expression::Int(7, Type::Unsigned(1)))],
                            kind: xor.clone(),
                        },
                    },
                    Statement::Bind {
                        name: "view".to_owned(),
                        mutable: false,
                        kind: traitref.clone(),
                        value: Expression::Coerce {
                            value: Box::new(Expression::Borrow {
                                mutable: false,
                                target: Box::new(Expression::Local("item".to_owned(), xor)),
                                kind: ptr,
                            }),
                            table: vtable.clone(),
                            kind: traitref.clone(),
                        },
                    },
                    Statement::Result(Expression::Virtual {
                        receiver: Box::new(Expression::Local("view".to_owned(), traitref)),
                        slot: 0,
                        arguments: vec![Expression::Bytes(asset.clone())],
                        kind: Type::Unsigned(8),
                    }),
                ],
            },
        ],
        entry: None,
    };
    let plan = compile(&unit).expect("compile shared vtable unit");
    assert_eq!(plan.mode, Mode::Shared);
    assert_eq!(plan.entry, 0);
    assert!(
        plan.exports
            .iter()
            .any(|s| s.name == vtable && !s.code && s.size == 8)
    );
    assert!(
        plan.exports
            .iter()
            .any(|s| s.name == asset && !s.code && s.size == 4)
    );
    assert!(
        plan.relocations
            .iter()
            .any(|r| r.symbol == method && r.kind == Kind::Absolute64)
    );
}
