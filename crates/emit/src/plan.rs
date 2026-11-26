use crate::encode::{Condition, Encoder, Width};
use crate::{Fault, Register};
use simplex_elf::{Kind, Mode, Plan, Relocation, Symbol};
use simplex_hir::{Routine, Type, Unit};
use simplex_runtime::{Hook, stubs};

pub const ALLOC: &str = "sx$emit$alloc";
pub const FREE: &str = "sx$emit$free";

#[derive(Default)]
pub struct Linker {
    pub code: Vec<u8>,
    pub data: Vec<u8>,
    pub exports: Vec<Symbol>,
    pub relocations: Vec<Relocation>,
    literals: u32,
}

impl Linker {
    pub fn new() -> Self {
        Self::default()
    }

    fn pad(buf: &mut Vec<u8>, align: usize, fill: u8) {
        while !buf.len().is_multiple_of(align) {
            buf.push(fill);
        }
    }

    pub fn section(&mut self, name: &str, bytes: &[u8]) {
        Self::pad(&mut self.data, 8, 0);
        let offset = self.data.len() as u64;
        let size = bytes.len() as u64;
        self.data.extend_from_slice(bytes);
        if bytes.is_empty() {
            self.data.push(0);
        }
        self.exports.push(Symbol {
            name: name.to_owned(),
            offset,
            size,
            code: false,
        });
    }

    pub fn literal(&mut self, bytes: &[u8]) -> String {
        let index = self.literals;
        self.literals += 1;
        let name = format!("sx$lit${index}");
        self.section(&name, bytes);
        name
    }

    pub fn table(&mut self, name: &str, methods: &[String]) {
        Self::pad(&mut self.data, 8, 0);
        let offset = self.data.len() as u64;
        let size = (methods.len() * 8) as u64;
        for (idx, method) in methods.iter().enumerate() {
            let slot = offset + (idx * 8) as u64;
            self.data.extend_from_slice(&[0; 8]);
            self.relocations.push(Relocation {
                offset: slot,
                symbol: method.clone(),
                kind: Kind::Absolute64,
                addend: 0,
            });
        }
        if methods.is_empty() {
            self.data.extend_from_slice(&[0; 8]);
        }
        self.exports.push(Symbol {
            name: name.to_owned(),
            offset,
            size,
            code: false,
        });
    }

    pub fn function(&mut self, name: &str, bytes: &[u8], relocs: Vec<Relocation>) -> u64 {
        Self::pad(&mut self.code, 16, 0x90);
        let offset = self.code.len() as u64;
        self.code.extend_from_slice(bytes);
        for mut item in relocs {
            item.offset += offset;
            self.relocations.push(item);
        }
        self.exports.push(Symbol {
            name: name.to_owned(),
            offset,
            size: bytes.len() as u64,
            code: true,
        });
        offset
    }

    pub fn startup(
        &mut self,
        entry: &str,
        routine: Option<&Routine>,
        start: bool,
    ) -> Result<u64, Fault> {
        let mut enc = Encoder::new();
        enc.constant(Register::Rbp, 0);
        enc.align();
        if start {
            enc.constant(Register::Rdi, 0);
            enc.call(Hook::Start.symbol(), false);
        }
        enc.call(entry, false);
        let output = match routine {
            Some(r) if r.asynchronous => {
                enc.copy(Register::Rdi, Register::Rax);
                enc.call(Hook::Await.symbol(), false);
                match &r.output {
                    Type::Fiber(inner) => (**inner).clone(),
                    other => other.clone(),
                }
            }
            Some(r) => match &r.output {
                Type::Fiber(inner) => {
                    enc.copy(Register::Rdi, Register::Rax);
                    enc.call(Hook::Await.symbol(), false);
                    (**inner).clone()
                }
                other => other.clone(),
            },
            None => Type::Signed(8),
        };
        exit(&mut enc, &output);
        enc.call(Hook::Exit.symbol(), false);
        enc.bytes.extend_from_slice(&[0x0F, 0x0B]);
        let (bytes, relocs) = enc.finish()?;
        Ok(self.function("_start", &bytes, relocs))
    }

    pub fn helpers(&mut self) -> Result<(), Fault> {
        let (ab, ar) = allocator()?;
        self.function(ALLOC, &ab, ar);
        let (fb, fr) = releaser()?;
        self.function(FREE, &fb, fr);
        for stub in stubs() {
            self.function(stub.hook.symbol(), &stub.code, Vec::new());
        }
        Ok(())
    }

    pub fn assemble(self, unit: &Unit, entry: u64) -> Plan {
        let dynamic = !unit.libraries.is_empty() || !unit.imports.is_empty();
        let mode = Mode::select(unit.shared, dynamic);
        let mut needed = Vec::new();
        for lib in &unit.libraries {
            if !needed.contains(lib) {
                needed.push(lib.clone());
            }
        }
        for imp in &unit.imports {
            if let Some(lib) = &imp.library
                && !needed.contains(lib)
            {
                needed.push(lib.clone());
            }
        }
        let mut imports = Vec::new();
        for imp in &unit.imports {
            if !imports.contains(&imp.symbol) {
                imports.push(imp.symbol.clone());
            }
        }
        Plan {
            mode,
            entry,
            needed,
            code: self.code,
            data: self.data,
            exports: self.exports,
            imports,
            relocations: self.relocations,
        }
    }
}

fn exit(enc: &mut Encoder, output: &Type) {
    match output {
        Type::Void => enc.constant(Register::Rdi, 0),
        Type::Bool | Type::Unsigned(1) => enc.extend(Register::Rdi, Register::Rax, Width::Byte),
        Type::Unsigned(2) => enc.extend(Register::Rdi, Register::Rax, Width::Word),
        Type::Unsigned(4) => enc.extend(Register::Rdi, Register::Rax, Width::Double),
        Type::Signed(1) => enc.widen(Register::Rdi, Register::Rax, Width::Byte),
        Type::Signed(2) => enc.widen(Register::Rdi, Register::Rax, Width::Word),
        Type::Signed(4) => enc.widen(Register::Rdi, Register::Rax, Width::Double),
        _ => enc.copy(Register::Rdi, Register::Rax),
    }
}

fn allocator() -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
    let mut enc = Encoder::new();
    let ready = enc.fresh();
    enc.adjust(Register::Rdi, 4095);
    enc.bytes
        .extend_from_slice(&[0x48, 0x81, 0xE7, 0x00, 0xF0, 0xFF, 0xFF]);
    enc.test(Register::Rdi);
    enc.branch(Condition::Distinct, ready);
    enc.constant(Register::Rdi, 4096);
    enc.bind(ready);
    enc.copy(Register::Rsi, Register::Rdi);
    enc.constant(Register::Rdi, 0);
    enc.constant(Register::Rdx, 3);
    enc.constant(Register::R10, 0x22);
    enc.constant(Register::R8, -1);
    enc.constant(Register::R9, 0);
    enc.constant(Register::Rax, 9);
    enc.bytes.extend_from_slice(&[0x0F, 0x05]);
    enc.leave();
    enc.finish()
}

fn releaser() -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
    let mut enc = Encoder::new();
    let done = enc.fresh();
    enc.test(Register::Rdi);
    enc.branch(Condition::Equal, done);
    enc.adjust(Register::Rsi, 4095);
    enc.bytes
        .extend_from_slice(&[0x48, 0x81, 0xE6, 0x00, 0xF0, 0xFF, 0xFF]);
    enc.constant(Register::Rax, 11);
    enc.bytes.extend_from_slice(&[0x0F, 0x05]);
    enc.bind(done);
    enc.leave();
    enc.finish()
}
