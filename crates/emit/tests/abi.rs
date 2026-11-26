use simplex_elf::{Kind, Mode};
use simplex_emit::compile;
use simplex_hir::{
    Catalog, Expression, Field, Footprint, Import, Layout, Mangle, Routine, Statement, Type, Unit,
};

fn record(name: &str, size: u64, fields: Vec<(&str, u64, Type)>) -> Layout {
    Layout {
        name: name.to_owned(),
        footprint: Footprint::aggregate(size, 8),
        fields: fields
            .into_iter()
            .map(|(n, o, k)| Field {
                name: n.to_owned(),
                offset: o,
                kind: k,
            })
            .collect(),
    }
}

#[test]
fn passing() {
    let mut catalog = Catalog::default();
    catalog.records.insert(
        "Span".to_owned(),
        record(
            "Span",
            16,
            vec![
                ("offset", 0, Type::Unsigned(8)),
                ("length", 8, Type::Unsigned(8)),
            ],
        ),
    );
    catalog.records.insert(
        "Packet".to_owned(),
        record(
            "Packet",
            24,
            vec![
                ("first", 0, Type::Unsigned(8)),
                ("second", 8, Type::Unsigned(8)),
                ("third", 16, Type::Unsigned(8)),
            ],
        ),
    );
    let pair = Mangle::symbol("app", "main", "pair");
    let wide = Mangle::symbol("app", "main", "wide");
    let main = Mangle::symbol("app", "main", "main");
    let span = Type::Record("Span".to_owned());
    let packet = Type::Record("Packet".to_owned());
    let routines = vec![
        Routine {
            symbol: pair.clone(),
            exported: false,
            asynchronous: false,
            inputs: vec![("item".to_owned(), span.clone())],
            output: span.clone(),
            body: vec![Statement::Result(Expression::Local(
                "item".to_owned(),
                span.clone(),
            ))],
        },
        Routine {
            symbol: wide.clone(),
            exported: false,
            asynchronous: false,
            inputs: vec![("pkt".to_owned(), packet.clone())],
            output: packet.clone(),
            body: vec![Statement::Result(Expression::Local(
                "pkt".to_owned(),
                packet.clone(),
            ))],
        },
        Routine {
            symbol: main.clone(),
            exported: true,
            asynchronous: false,
            inputs: Vec::new(),
            output: Type::Unsigned(8),
            body: vec![
                Statement::Bind {
                    name: "s".to_owned(),
                    mutable: false,
                    kind: span.clone(),
                    value: Expression::Call {
                        symbol: pair,
                        arguments: vec![Expression::Record {
                            name: "Span".to_owned(),
                            fields: vec![
                                ("offset".to_owned(), Expression::Int(10, Type::Unsigned(8))),
                                ("length".to_owned(), Expression::Int(32, Type::Unsigned(8))),
                            ],
                            kind: span.clone(),
                        }],
                        kind: span.clone(),
                    },
                },
                Statement::Bind {
                    name: "p".to_owned(),
                    mutable: false,
                    kind: packet.clone(),
                    value: Expression::Call {
                        symbol: wide,
                        arguments: vec![Expression::Record {
                            name: "Packet".to_owned(),
                            fields: vec![
                                ("first".to_owned(), Expression::Int(1, Type::Unsigned(8))),
                                ("second".to_owned(), Expression::Int(2, Type::Unsigned(8))),
                                ("third".to_owned(), Expression::Int(3, Type::Unsigned(8))),
                            ],
                            kind: packet.clone(),
                        }],
                        kind: packet.clone(),
                    },
                },
                Statement::Result(Expression::Binary {
                    operator: "+".to_owned(),
                    left: Box::new(Expression::Field {
                        base: Box::new(Expression::Local("s".to_owned(), span)),
                        name: "length".to_owned(),
                        offset: 8,
                        kind: Type::Unsigned(8),
                    }),
                    right: Box::new(Expression::Field {
                        base: Box::new(Expression::Local("p".to_owned(), packet)),
                        name: "third".to_owned(),
                        offset: 16,
                        kind: Type::Unsigned(8),
                    }),
                    kind: Type::Unsigned(8),
                }),
            ],
        },
    ];
    let unit = Unit {
        namespace: "app".to_owned(),
        shared: false,
        libraries: Vec::new(),
        imports: Vec::new(),
        assets: Vec::new(),
        tables: Vec::new(),
        catalog,
        routines,
        entry: Some(main),
    };
    let plan = compile(&unit).expect("compile standalone abi unit");
    assert_eq!(plan.mode, Mode::Standalone);
    assert!(plan.exports.iter().any(|s| s.name == "_start" && s.code));
    assert!(
        plan.exports
            .iter()
            .any(|s| s.name == "sx$rt$exit" && s.code)
    );
}

#[test]
fn foreign() {
    let main = Mangle::symbol("app", "main", "main");
    let unit = Unit {
        namespace: "app".to_owned(),
        shared: false,
        libraries: vec!["libc.so.6".to_owned()],
        imports: vec![Import {
            local: "emit".to_owned(),
            symbol: "write".to_owned(),
            library: Some("libc.so.6".to_owned()),
            inputs: vec![
                Type::Signed(4),
                Type::Pointer(Box::new(Type::Unsigned(1))),
                Type::Unsigned(8),
            ],
            output: Type::Signed(8),
        }],
        assets: Vec::new(),
        tables: Vec::new(),
        catalog: Catalog::default(),
        routines: vec![Routine {
            symbol: main.clone(),
            exported: true,
            asynchronous: false,
            inputs: Vec::new(),
            output: Type::Signed(8),
            body: vec![Statement::Result(Expression::Call {
                symbol: "write".to_owned(),
                arguments: vec![
                    Expression::Int(1, Type::Signed(4)),
                    Expression::Int(0, Type::Pointer(Box::new(Type::Unsigned(1)))),
                    Expression::Int(0, Type::Unsigned(8)),
                ],
                kind: Type::Signed(8),
            })],
        }],
        entry: Some(main),
    };
    let plan = compile(&unit).expect("compile dynamic ffi unit");
    assert_eq!(plan.mode, Mode::Dynamic);
    assert_eq!(plan.needed, vec!["libc.so.6".to_owned()]);
    assert_eq!(plan.imports, vec!["write".to_owned()]);
    assert!(
        plan.relocations
            .iter()
            .any(|r| r.symbol == "write" && r.kind == Kind::Procedure32)
    );
}
