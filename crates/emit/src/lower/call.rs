use super::abi::{Signature, Value};
use crate::encode::media::Precision;
use crate::machine::linear::{Callee, Invocation, Outgoing, Parameter, Procedure, Step};
use simplex_hir::{Passing, Type};

pub struct Entry {
    pub hidden: Option<u32>,
    pub values: Vec<Value>,
}

#[derive(Default)]
struct Staging {
    general: usize,
    media: usize,
    offset: u32,
    registers: Vec<u32>,
    floats: Vec<(u32, Precision)>,
    stack: Vec<Outgoing>,
}

impl Staging {
    fn new(general: usize) -> Self {
        Self {
            general,
            ..Self::default()
        }
    }

    fn align(&mut self, align: u32) {
        let mask = align.max(8) - 1;
        self.offset = (self.offset + mask) & !mask;
    }

    fn single(&mut self, src: u32) {
        match self.general < 6 {
            true => {
                self.general += 1;
                self.registers.push(src);
            }
            false => {
                self.stack.push(Outgoing::Scalar {
                    src,
                    offset: self.offset,
                });
                self.offset += 8;
            }
        }
    }

    fn pair(&mut self, first: u32, second: u32) {
        match self.general + 2 <= 6 {
            true => {
                self.general += 2;
                self.registers.push(first);
                self.registers.push(second);
            }
            false => {
                self.stack.push(Outgoing::Scalar {
                    src: first,
                    offset: self.offset,
                });
                self.stack.push(Outgoing::Scalar {
                    src: second,
                    offset: self.offset + 8,
                });
                self.offset += 16;
            }
        }
    }
}

impl Signature<'_> {
    pub fn enter(&self, proc: &mut Procedure, inputs: &[(String, Type)], output: &Type) -> Entry {
        let indirect = self.footprint(output).passing == Passing::Indirect;
        let hidden = indirect.then(|| {
            let dst = proc.value();
            proc.parameters.push(Parameter::Register { dst, index: 0 });
            dst
        });
        let mut cursor = Staging::new(usize::from(indirect));
        let mut values = Vec::with_capacity(inputs.len());
        for (_, kind) in inputs {
            values.push(self.receive(proc, &mut cursor, kind));
        }
        Entry { hidden, values }
    }

    fn receive(&self, proc: &mut Procedure, cursor: &mut Staging, kind: &Type) -> Value {
        let fp = self.footprint(kind);
        match fp.passing {
            Passing::Void => Value::Void,
            Passing::Register => Value::Scalar(self.word(proc, cursor, kind)),
            Passing::Pair if cursor.general + 2 <= 6 => {
                let first = Self::register(proc, cursor);
                let second = Self::register(proc, cursor);
                Value::Pair(first, second)
            }
            Passing::Pair => {
                let first = Self::spill(proc, cursor);
                let second = Self::spill(proc, cursor);
                Value::Pair(first, second)
            }
            Passing::Indirect => {
                cursor.align(fp.align as u32);
                let slot = proc.reserve(fp.size as u32, fp.align as u32);
                proc.parameters.push(Parameter::Block {
                    slot,
                    offset: cursor.offset as i32,
                    bytes: fp.size as u32,
                });
                cursor.offset += (fp.size as u32 + 7) & !7;
                Value::Address(Self::base(proc, slot, 0))
            }
        }
    }

    fn word(&self, proc: &mut Procedure, cursor: &mut Staging, kind: &Type) -> u32 {
        let raw = match cursor.general < 6 {
            true => Self::register(proc, cursor),
            false => Self::spill(proc, cursor),
        };
        Self::normalize(proc, raw, kind)
    }

    fn register(proc: &mut Procedure, cursor: &mut Staging) -> u32 {
        let dst = proc.value();
        let index = cursor.general;
        cursor.general += 1;
        proc.parameters.push(Parameter::Register { dst, index });
        dst
    }

    fn spill(proc: &mut Procedure, cursor: &mut Staging) -> u32 {
        let dst = proc.value();
        let offset = cursor.offset as i32;
        cursor.offset += 8;
        proc.parameters.push(Parameter::Stack { dst, offset });
        dst
    }

    pub fn invoke(
        &self,
        proc: &mut Procedure,
        callee: Callee,
        args: &[(Value, Type)],
        output: &Type,
    ) -> Value {
        let out = self.footprint(output);
        let mut stage = Staging::new(0);
        let indirect = (out.passing == Passing::Indirect).then(|| {
            let slot = proc.reserve(out.size as u32, out.align as u32);
            let ptr = Self::base(proc, slot, 0);
            stage.single(ptr);
            ptr
        });
        for (val, kind) in args {
            self.pass(proc, (*val, kind), &mut stage);
        }
        let media = match (self.foreign, output) {
            (true, Type::Float(w)) => Some(Precision::from(*w)),
            _ => None,
        };
        let (first, second, result) = self.capture(proc, indirect, output);
        proc.emit(Step::Invoke(Invocation {
            callee,
            registers: stage.registers,
            floats: stage.floats,
            stack: stage.stack,
            space: stage.offset,
            first,
            second,
            media,
        }));
        match result {
            Value::Scalar(v) => Value::Scalar(Self::normalize(proc, v, output)),
            other => other,
        }
    }

    fn pass(&self, proc: &mut Procedure, arg: (Value, &Type), stage: &mut Staging) {
        let (val, kind) = arg;
        if let (true, Type::Float(w), Value::Scalar(src)) = (self.foreign, kind, val)
            && stage.media < 8
        {
            stage.media += 1;
            stage.floats.push((src, Precision::from(*w)));
            return;
        }
        let fp = self.footprint(kind);
        match (fp.passing, val) {
            (Passing::Void, _) => {}
            (Passing::Register, Value::Scalar(src)) => {
                stage.single(Self::normalize(proc, src, kind));
            }
            (Passing::Register, Value::Address(ptr)) => {
                if let Value::Scalar(src) = self.load(proc, ptr, kind) {
                    stage.single(src);
                }
            }
            (Passing::Pair, Value::Pair(a, b)) => stage.pair(a, b),
            (Passing::Pair, Value::Address(ptr)) => {
                if let Value::Pair(a, b) = self.load(proc, ptr, kind) {
                    stage.pair(a, b);
                }
            }
            (Passing::Indirect, other) => {
                let base = self.materialize(proc, other, kind);
                stage.align(fp.align as u32);
                stage.stack.push(Outgoing::Block {
                    base,
                    offset: stage.offset,
                    bytes: fp.size as u32,
                });
                stage.offset += (fp.size as u32 + 7) & !7;
            }
            _ => {}
        }
    }

    fn capture(
        &self,
        proc: &mut Procedure,
        indirect: Option<u32>,
        output: &Type,
    ) -> (Option<u32>, Option<u32>, Value) {
        let fp = self.footprint(output);
        match (fp.passing, indirect) {
            (Passing::Void, _) => (None, None, Value::Void),
            (Passing::Register, _) => {
                let dst = proc.value();
                (Some(dst), None, Value::Scalar(dst))
            }
            (Passing::Pair, _) => {
                let first = proc.value();
                let second = proc.value();
                (Some(first), Some(second), Value::Pair(first, second))
            }
            (Passing::Indirect, Some(ptr)) => (None, None, Value::Address(ptr)),
            _ => (None, None, Value::Void),
        }
    }
}
