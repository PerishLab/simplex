use simplex_elf::Mode;
use simplex_emit::compile;
use simplex_hir::{
    Arm, Case, Catalog, Expression, Field, Footprint, Mangle, Routine, Statement, Tagged, Type,
    Unit,
};

#[test]
fn control() {
    let mut catalog = Catalog::default();
    catalog.choices.insert(
        "Outcome".to_owned(),
        Tagged {
            name: "Outcome".to_owned(),
            discriminant: 1,
            footprint: Footprint::pair(),
            arms: vec![
                Arm {
                    name: "empty".to_owned(),
                    tag: 0,
                    payloads: Vec::new(),
                },
                Arm {
                    name: "some".to_owned(),
                    tag: 1,
                    payloads: vec![Field {
                        name: "0".to_owned(),
                        offset: 8,
                        kind: Type::Signed(8),
                    }],
                },
            ],
        },
    );
    let main = Mangle::symbol("app", "main", "main");
    let choice = Type::Choice("Outcome".to_owned());
    let slice = Type::Slice(Box::new(Type::Unsigned(1)));
    let body = vec![
        Statement::Bind {
            name: "item".to_owned(),
            mutable: false,
            kind: choice.clone(),
            value: Expression::Variant {
                choice: "Outcome".to_owned(),
                arm: "some".to_owned(),
                tag: 1,
                payloads: vec![Expression::Cast {
                    value: Box::new(Expression::Float(42.9_f64.to_bits(), Type::Float(8))),
                    kind: Type::Signed(8),
                }],
                kind: choice.clone(),
            },
        },
        Statement::Bind {
            name: "extracted".to_owned(),
            mutable: false,
            kind: Type::Signed(8),
            value: Expression::Switch {
                subject: Box::new(Expression::Local("item".to_owned(), choice)),
                cases: vec![
                    Case {
                        tags: vec![0],
                        bindings: Vec::new(),
                        body: Expression::Int(0, Type::Signed(8)),
                    },
                    Case {
                        tags: vec![1],
                        bindings: vec![(
                            "val".to_owned(),
                            Field {
                                name: "0".to_owned(),
                                offset: 8,
                                kind: Type::Signed(8),
                            },
                        )],
                        body: Expression::Local("val".to_owned(), Type::Signed(8)),
                    },
                ],
                kind: Type::Signed(8),
            },
        },
        Statement::Bind {
            name: "same".to_owned(),
            mutable: false,
            kind: Type::Bool,
            value: Expression::Binary {
                operator: "==".to_owned(),
                left: Box::new(Expression::Window {
                    base: Box::new(Expression::Bytes("simplex".to_owned())),
                    start: Box::new(Expression::Int(0, Type::Unsigned(8))),
                    limit: Box::new(Expression::Int(3, Type::Unsigned(8))),
                    kind: slice,
                }),
                right: Box::new(Expression::Bytes("sim".to_owned())),
                kind: Type::Bool,
            },
        },
        Statement::Result(Expression::Branch {
            condition: Box::new(Expression::Local("same".to_owned(), Type::Bool)),
            then: Box::new(Expression::Loop {
                body: vec![Statement::Break(Some(Expression::Local(
                    "extracted".to_owned(),
                    Type::Signed(8),
                )))],
                kind: Type::Signed(8),
            }),
            otherwise: Box::new(Expression::Int(-1, Type::Signed(8))),
            kind: Type::Signed(8),
        }),
    ];
    let unit = Unit {
        namespace: "app".to_owned(),
        shared: false,
        libraries: Vec::new(),
        imports: Vec::new(),
        assets: Vec::new(),
        tables: Vec::new(),
        catalog,
        routines: vec![Routine {
            symbol: main.clone(),
            exported: true,
            asynchronous: false,
            inputs: Vec::new(),
            output: Type::Signed(8),
            body,
        }],
        entry: Some(main),
    };
    let plan = compile(&unit).expect("compile control unit");
    assert_eq!(plan.mode, Mode::Standalone);
    assert!(plan.exports.iter().any(|s| s.name == "sx$lit$0" && !s.code));
    assert!(plan.exports.iter().any(|s| s.name == "sx$lit$1" && !s.code));
}
