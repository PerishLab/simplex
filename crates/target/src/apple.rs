use crate::Fault;
use simplex_ir::Module;
use simplex_object::Artifact;

pub fn compile(name: &str, module: &Module) -> Result<Artifact, Fault> {
    let _ = (name, module);
    Err(Fault::Unsupported("apple".to_owned()))
}
