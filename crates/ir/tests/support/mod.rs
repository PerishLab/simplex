pub mod builder;
pub mod engine;
pub mod math;

use simplex_ir::{Module, Operation, Type};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Void,
    Word(u64),
    Pair(u64, u64),
    Bytes(Vec<u8>),
}

impl Value {
    pub fn word(&self) -> u64 {
        match self {
            Self::Word(v) => *v,
            other => panic!("expected word, got {other:?}"),
        }
    }

    pub fn pair(&self) -> (u64, u64) {
        match self {
            Self::Pair(a, b) => (*a, *b),
            other => panic!("expected pair, got {other:?}"),
        }
    }

    pub fn encode(&self, kind: &Type) -> Vec<u8> {
        match (self, kind) {
            (Self::Word(v), Type::Integer { bits, .. }) => {
                let len = (*bits as usize).div_ceil(8);
                v.to_le_bytes()[..len].to_vec()
            }
            (Self::Word(v), Type::Pointer) => v.to_le_bytes().to_vec(),
            (Self::Word(v), Type::Float { bits: 32 }) => (*v as u32).to_le_bytes().to_vec(),
            (Self::Word(v), Type::Float { bits: 64 }) => v.to_le_bytes().to_vec(),
            (Self::Pair(a, b), Type::Pair) => {
                let mut out = Vec::with_capacity(16);
                out.extend_from_slice(&a.to_le_bytes());
                out.extend_from_slice(&b.to_le_bytes());
                out
            }
            (Self::Bytes(b), Type::Aggregate { size, .. }) => {
                assert_eq!(b.len(), *size as usize);
                b.clone()
            }
            other => panic!("cannot encode {other:?}"),
        }
    }

    pub fn decode(bytes: &[u8], kind: &Type) -> Self {
        match kind {
            Type::Integer { .. } | Type::Pointer | Type::Float { .. } => {
                let mut buf = [0u8; 8];
                buf[..bytes.len()].copy_from_slice(bytes);
                Self::Word(u64::from_le_bytes(buf))
            }
            Type::Pair => {
                let mut lo = [0u8; 8];
                let mut hi = [0u8; 8];
                lo.copy_from_slice(&bytes[..8]);
                hi.copy_from_slice(&bytes[8..16]);
                Self::Pair(u64::from_le_bytes(lo), u64::from_le_bytes(hi))
            }
            Type::Aggregate { .. } => Self::Bytes(bytes.to_vec()),
            Type::Void => Self::Void,
        }
    }
}

pub fn pure(module: &Module) -> bool {
    !module.functions.iter().any(|f| f.asynchronous)
        && !module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.body)
            .any(|i| {
                matches!(
                    i.op,
                    Operation::Spawn { .. }
                        | Operation::Yield
                        | Operation::Await { .. }
                        | Operation::All { .. }
                        | Operation::Race { .. }
                )
            })
}
