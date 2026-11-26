use crate::encode::Width;
use crate::machine::linear::{Access, Binary, Ending, Procedure, Slot, Step, Unary};
use simplex_hir::{Catalog, Footprint, Passing, Type};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Void,
    Scalar(u32),
    Pair(u32, u32),
    Address(u32),
}

pub struct Signature<'a> {
    pub catalog: &'a Catalog,
    pub foreign: bool,
}

pub struct Write {
    pub ptr: u32,
    pub offset: i32,
    pub bytes: u64,
}

impl<'a> Signature<'a> {
    pub const fn new(catalog: &'a Catalog, foreign: bool) -> Self {
        Self { catalog, foreign }
    }

    pub fn footprint(&self, kind: &Type) -> Footprint {
        kind.footprint(self.catalog)
    }

    pub fn load(&self, proc: &mut Procedure, ptr: u32, kind: &Type) -> Value {
        let fp = self.footprint(kind);
        match fp.passing {
            Passing::Void => Value::Void,
            Passing::Register => Value::Scalar(self.scalar(proc, ptr, kind)),
            Passing::Pair => {
                let first = self.fetch(proc, ptr, 0, 8);
                let second = self.fetch(proc, ptr, 8, fp.size.saturating_sub(8));
                Value::Pair(first, second)
            }
            Passing::Indirect => {
                let slot = proc.reserve(fp.size as u32, fp.align as u32);
                let dst = Self::base(proc, slot, 0);
                proc.emit(Step::Copy {
                    dst,
                    src: ptr,
                    bytes: fp.size as u32,
                });
                Value::Address(dst)
            }
        }
    }

    fn scalar(&self, proc: &mut Procedure, ptr: u32, kind: &Type) -> u32 {
        match kind {
            Type::Signed(b @ (1 | 2 | 4)) => {
                let dst = proc.value();
                proc.emit(Step::Load {
                    dst,
                    base: ptr,
                    offset: 0,
                    access: Access {
                        width: Width::from(u64::from(*b)),
                        signed: true,
                    },
                });
                dst
            }
            Type::Bool | Type::Unsigned(1) => self.fetch(proc, ptr, 0, 1),
            Type::Unsigned(2) => self.fetch(proc, ptr, 0, 2),
            Type::Unsigned(4) | Type::Float(4) => self.fetch(proc, ptr, 0, 4),
            _ => self.fetch(proc, ptr, 0, self.footprint(kind).size),
        }
    }

    pub fn fetch(&self, proc: &mut Procedure, ptr: u32, offset: i32, bytes: u64) -> u32 {
        match bytes {
            0 => Self::constant(proc, 0),
            1 | 2 | 4 | 8 => {
                let dst = proc.value();
                proc.emit(Step::Load {
                    dst,
                    base: ptr,
                    offset,
                    access: Access {
                        width: Width::from(bytes),
                        signed: false,
                    },
                });
                dst
            }
            _ => {
                let scratch = proc.reserve(8, 8);
                let tmp = Self::base(proc, scratch, 0);
                proc.emit(Step::Zero { dst: tmp, bytes: 8 });
                let src = Self::displace(proc, ptr, offset);
                proc.emit(Step::Copy {
                    dst: tmp,
                    src,
                    bytes: bytes as u32,
                });
                self.fetch(proc, tmp, 0, 8)
            }
        }
    }

    pub fn store(&self, proc: &mut Procedure, ptr: u32, val: Value, kind: &Type) {
        let fp = self.footprint(kind);
        match (fp.passing, val) {
            (Passing::Void, _) | (_, Value::Void) => {}
            (Passing::Register, Value::Scalar(src)) => {
                let target = Write {
                    ptr,
                    offset: 0,
                    bytes: fp.size,
                };
                self.write(proc, target, src);
            }
            (Passing::Pair, Value::Pair(first, second)) => {
                self.write(
                    proc,
                    Write {
                        ptr,
                        offset: 0,
                        bytes: 8,
                    },
                    first,
                );
                self.write(
                    proc,
                    Write {
                        ptr,
                        offset: 8,
                        bytes: fp.size.saturating_sub(8),
                    },
                    second,
                );
            }
            (_, Value::Address(src)) => {
                proc.emit(Step::Copy {
                    dst: ptr,
                    src,
                    bytes: fp.size as u32,
                });
            }
            _ => {}
        }
    }

    pub fn write(&self, proc: &mut Procedure, target: Write, src: u32) {
        match target.bytes {
            0 => {}
            1 | 2 | 4 | 8 => proc.emit(Step::Store {
                base: target.ptr,
                offset: target.offset,
                src,
                width: Width::from(target.bytes),
            }),
            _ => {
                let scratch = proc.reserve(8, 8);
                let tmp = Self::base(proc, scratch, 0);
                proc.emit(Step::Store {
                    base: tmp,
                    offset: 0,
                    src,
                    width: Width::Quad,
                });
                let dst = Self::displace(proc, target.ptr, target.offset);
                proc.emit(Step::Copy {
                    dst,
                    src: tmp,
                    bytes: target.bytes as u32,
                });
            }
        }
    }

    pub fn materialize(&self, proc: &mut Procedure, val: Value, kind: &Type) -> u32 {
        match val {
            Value::Address(ptr) => ptr,
            other => {
                let fp = self.footprint(kind);
                let span = fp.size.max(8) as u32;
                let slot = proc.reserve(span, fp.align.max(8) as u32);
                let ptr = Self::base(proc, slot, 0);
                proc.emit(Step::Zero {
                    dst: ptr,
                    bytes: span,
                });
                self.store(proc, ptr, other, kind);
                ptr
            }
        }
    }

    pub fn base(proc: &mut Procedure, slot: Slot, delta: i32) -> u32 {
        let dst = proc.value();
        proc.emit(Step::Slot { dst, slot, delta });
        dst
    }

    pub fn displace(proc: &mut Procedure, base: u32, delta: i32) -> u32 {
        if delta == 0 {
            return base;
        }
        let off = Self::constant(proc, i64::from(delta));
        let dst = proc.value();
        proc.emit(Step::Binary {
            kind: Binary::Add,
            dst,
            lhs: base,
            rhs: off,
        });
        dst
    }

    pub fn constant(proc: &mut Procedure, value: i64) -> u32 {
        let dst = proc.value();
        proc.emit(Step::Constant { dst, value });
        dst
    }

    pub fn normalize(proc: &mut Procedure, src: u32, kind: &Type) -> u32 {
        let op = match kind {
            Type::Bool | Type::Unsigned(1) => Some(Unary::Extend(Width::Byte)),
            Type::Unsigned(2) => Some(Unary::Extend(Width::Word)),
            Type::Unsigned(4) => Some(Unary::Extend(Width::Double)),
            Type::Signed(1) => Some(Unary::Widen(Width::Byte)),
            Type::Signed(2) => Some(Unary::Widen(Width::Word)),
            Type::Signed(4) => Some(Unary::Widen(Width::Double)),
            _ => None,
        };
        match op {
            Some(unary) => {
                let dst = proc.value();
                proc.emit(Step::Unary {
                    kind: unary,
                    dst,
                    src,
                });
                dst
            }
            None => src,
        }
    }

    pub fn finish(&self, proc: &mut Procedure, hidden: Option<u32>, val: Value, output: &Type) {
        let fp = self.footprint(output);
        let ending = match (fp.passing, val, hidden) {
            (Passing::Void, _, _) => Ending::Void,
            (Passing::Register, Value::Scalar(v), _) => {
                Ending::Single(Self::normalize(proc, v, output))
            }
            (Passing::Register, Value::Address(ptr), _) => match self.load(proc, ptr, output) {
                Value::Scalar(v) => Ending::Single(v),
                _ => Ending::Void,
            },
            (Passing::Pair, Value::Pair(a, b), _) => Ending::Pair(a, b),
            (Passing::Pair, Value::Address(ptr), _) => match self.load(proc, ptr, output) {
                Value::Pair(a, b) => Ending::Pair(a, b),
                _ => Ending::Void,
            },
            (Passing::Indirect, other, Some(target)) => {
                let source = self.materialize(proc, other, output);
                Ending::Indirect {
                    target,
                    source,
                    bytes: fp.size as u32,
                }
            }
            _ => Ending::Void,
        };
        proc.emit(Step::Finish(ending));
    }
}
