use simplex_elf::Mode;
use simplex_emit::compile;
use simplex_hir::{
    Catalog, Expression, Field, Footprint, Layout, Mangle, Routine, Statement, Type, Unit,
};

#[test]
fn fibers() {
    let mut catalog = Catalog::default();
    catalog.records.insert(
        "Guard".to_owned(),
        Layout {
            name: "Guard".to_owned(),
            footprint: Footprint::scalar(8),
            fields: vec![Field {
                name: "code".to_owned(),
                offset: 0,
                kind: Type::Unsigned(8),
            }],
        },
    );
    let dropfn = Mangle::symbol("app", "main", "drop");
    let worker = Mangle::symbol("app", "main", "worker");
    let main = Mangle::symbol("app", "main", "main");
    let guard = Type::Record("Guard".to_owned());
    let fiber = Type::Fiber(Box::new(Type::Unsigned(8)));
    let list = Type::List(Box::new(fiber.clone()));
    let outlist = Type::List(Box::new(Type::Unsigned(8)));
    let unit = Unit {
        namespace: "app".to_owned(),
        shared: false,
        libraries: Vec::new(),
        imports: Vec::new(),
        assets: Vec::new(),
        tables: Vec::new(),
        catalog,
        routines: vec![
            Routine {
                symbol: dropfn.clone(),
                exported: false,
                asynchronous: false,
                inputs: vec![("self".to_owned(), Type::Pointer(Box::new(guard.clone())))],
                output: Type::Void,
                body: Vec::new(),
            },
            Routine {
                symbol: worker.clone(),
                exported: false,
                asynchronous: true,
                inputs: vec![("seed".to_owned(), Type::Unsigned(8))],
                output: Type::Unsigned(8),
                body: vec![
                    Statement::Bind {
                        name: "g".to_owned(),
                        mutable: false,
                        kind: guard.clone(),
                        value: Expression::Record {
                            name: "Guard".to_owned(),
                            fields: vec![(
                                "code".to_owned(),
                                Expression::Local("seed".to_owned(), Type::Unsigned(8)),
                            )],
                            kind: guard,
                        },
                    },
                    Statement::Evaluate(Expression::Yield),
                    Statement::Drop {
                        name: "g".to_owned(),
                        symbol: dropfn.clone(),
                        borrowed: false,
                    },
                    Statement::Result(Expression::Local("seed".to_owned(), Type::Unsigned(8))),
                ],
            },
            Routine {
                symbol: main.clone(),
                exported: true,
                asynchronous: true,
                inputs: Vec::new(),
                output: Type::Unsigned(8),
                body: vec![
                    Statement::Bind {
                        name: "batch".to_owned(),
                        mutable: false,
                        kind: outlist.clone(),
                        value: Expression::All {
                            fibers: Box::new(Expression::List {
                                items: vec![
                                    Expression::Call {
                                        symbol: worker.clone(),
                                        arguments: vec![Expression::Int(20, Type::Unsigned(8))],
                                        kind: fiber.clone(),
                                    },
                                    Expression::Call {
                                        symbol: worker.clone(),
                                        arguments: vec![Expression::Int(22, Type::Unsigned(8))],
                                        kind: fiber.clone(),
                                    },
                                ],
                                element: fiber.clone(),
                                kind: list.clone(),
                            }),
                            element: Type::Unsigned(8),
                            kind: outlist.clone(),
                        },
                    },
                    Statement::Bind {
                        name: "fast".to_owned(),
                        mutable: false,
                        kind: Type::Unsigned(8),
                        value: Expression::Race {
                            fibers: Box::new(Expression::List {
                                items: vec![Expression::Call {
                                    symbol: worker.clone(),
                                    arguments: vec![Expression::Int(5, Type::Unsigned(8))],
                                    kind: fiber,
                                }],
                                element: Type::Fiber(Box::new(Type::Unsigned(8))),
                                kind: list,
                            }),
                            kind: Type::Unsigned(8),
                        },
                    },
                    Statement::Result(Expression::Binary {
                        operator: "+".to_owned(),
                        left: Box::new(Expression::Index {
                            base: Box::new(Expression::Local("batch".to_owned(), outlist)),
                            index: Box::new(Expression::Int(0, Type::Unsigned(8))),
                            kind: Type::Unsigned(8),
                        }),
                        right: Box::new(Expression::Local("fast".to_owned(), Type::Unsigned(8))),
                        kind: Type::Unsigned(8),
                    }),
                ],
            },
        ],
        entry: Some(main.clone()),
    };
    let plan = compile(&unit).expect("compile async fiber unit");
    assert_eq!(plan.mode, Mode::Standalone);
    let entry = format!("{worker}$entry");
    let clean = format!("{worker}$clean");
    assert!(plan.exports.iter().any(|s| s.name == worker && s.code));
    assert!(plan.exports.iter().any(|s| s.name == entry && s.code));
    assert!(plan.exports.iter().any(|s| s.name == clean && s.code));
    assert!(plan.relocations.iter().any(|r| r.symbol == "sx$rt$spawn"));
    assert!(plan.relocations.iter().any(|r| r.symbol == "sx$rt$await"));
    assert!(plan.relocations.iter().any(|r| r.symbol == "sx$rt$all"));
    assert!(plan.relocations.iter().any(|r| r.symbol == "sx$rt$race"));
    assert!(plan.relocations.iter().any(|r| r.symbol == dropfn));
}
