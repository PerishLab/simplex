use simplex_ir::{External, Linkage, Module, Type};
use simplex_target::{Platform, compile};

fn module() -> Module {
    Module {
        shared: true,
        entry: None,
        assets: Vec::new(),
        tables: Vec::new(),
        externals: vec![External {
            symbol: "sx$ext".to_owned(),
            linkage: Linkage::Dynamic("codec".to_owned()),
            inputs: Vec::new(),
            output: Type::Pointer,
        }],
        functions: Vec::new(),
    }
}

fn libraries(platform: Platform) -> Vec<String> {
    let art = compile(platform, "demo", &module()).expect("compile");
    art.imports.iter().map(|i| i.library.clone()).collect()
}

#[test]
fn linux() {
    assert!(libraries(Platform::Linux).contains(&"codec.so".to_owned()));
}

#[test]
fn windows() {
    assert!(libraries(Platform::Windows).contains(&"codec.dll".to_owned()));
}

#[test]
fn apple() {
    let want = "@executable_path/codec.dylib".to_owned();
    assert!(libraries(Platform::Apple).contains(&want));
}
