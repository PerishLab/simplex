mod settle;

use self::settle::{collect, epilogue};
use super::abi::{Signature, Value};
use super::{Async, Cleanup, Function, Local, Storage};
use crate::Fault;
use crate::encode::Width;
use crate::machine::linear::{Binary, Callee, Ending, Parameter, Procedure, Step};
use crate::plan::{ALLOC, FREE};
use simplex_hir::{Routine, Type, Unit};
use simplex_runtime::Hook;

pub struct Builder<'a> {
    pub unit: &'a Unit,
}

struct Capture<'a> {
    params: &'a [(String, Type, i32)],
    cleanups: &'a [Cleanup],
    total: i32,
}

pub fn unwrap(kind: &Type) -> Type {
    match kind {
        Type::Fiber(inner) => (**inner).clone(),
        other => other.clone(),
    }
}

impl<'a> Builder<'a> {
    pub fn synthesize(
        &self,
        routine: &Routine,
        literals: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<Vec<Procedure>, Fault> {
        let sig = Signature::new(&self.unit.catalog, false);
        let payload = unwrap(&routine.output);
        let mut cursor = 16i32;
        let mut params = Vec::with_capacity(routine.inputs.len());
        for (name, kind) in &routine.inputs {
            let fp = sig.footprint(kind);
            let align = fp.align.max(8) as i32;
            cursor = (cursor + align - 1) & !(align - 1);
            params.push((name.clone(), kind.clone(), cursor));
            cursor += fp.size.max(8) as i32;
        }
        let drops = collect(&routine.inputs, &routine.body);
        let mut cleanups = Vec::new();
        for (idx, (name, symbol, kind)) in drops.into_iter().take(62).enumerate() {
            let offset = match params.iter().find(|p| p.0 == name) {
                Some(p) => p.2,
                None => {
                    let fp = sig.footprint(&kind);
                    let align = fp.align.max(8) as i32;
                    cursor = (cursor + align - 1) & !(align - 1);
                    let off = cursor;
                    cursor += fp.size.max(8) as i32;
                    off
                }
            };
            cleanups.push(Cleanup {
                name,
                symbol,
                offset,
                bit: idx as u32,
            });
        }
        let total = ((cursor + 4095) & !4095).max(4096);
        let entry = format!("{}$entry", routine.symbol);
        let clean = format!("{}$clean", routine.symbol);
        let cap = Capture {
            params: &params,
            cleanups: &cleanups,
            total,
        };
        let spawn = self.wrapper(routine, (&entry, &clean), &cap);
        let body = self.worker(routine, literals, &cap)?;
        let drop = self.cleaner(&clean, &cleanups, total);
        let _ = payload;
        Ok(vec![spawn, body, drop])
    }

    fn wrapper(&self, routine: &Routine, names: (&str, &str), cap: &Capture<'_>) -> Procedure {
        let mut proc = Procedure::new(&routine.symbol);
        let sig = Signature::new(&self.unit.catalog, false);
        let out = Type::Fiber(Box::new(unwrap(&routine.output)));
        let entered = sig.enter(&mut proc, &routine.inputs, &out);
        let size = Signature::constant(&mut proc, i64::from(cap.total));
        let ptr = Type::Pointer(Box::new(Type::Unsigned(1)));
        let alloc = Callee::Direct {
            symbol: ALLOC.to_owned(),
            external: false,
        };
        let Value::Scalar(env) = sig.invoke(
            &mut proc,
            alloc,
            &[(Value::Scalar(size), Type::Unsigned(8))],
            &ptr,
        ) else {
            return proc;
        };
        for (idx, (_, kind, off)) in cap.params.iter().enumerate() {
            let dst = Signature::displace(&mut proc, env, *off);
            sig.store(&mut proc, dst, entered.values[idx], kind);
        }
        let ep = proc.value();
        proc.emit(Step::Symbol {
            dst: ep,
            name: names.0.to_owned(),
            external: false,
        });
        let cp = proc.value();
        proc.emit(Step::Symbol {
            dst: cp,
            name: names.1.to_owned(),
            external: false,
        });
        let spawn = Callee::Direct {
            symbol: Hook::Spawn.symbol().to_owned(),
            external: false,
        };
        let args = [
            (Value::Scalar(ep), ptr.clone()),
            (Value::Scalar(env), ptr.clone()),
            (Value::Scalar(cp), ptr),
        ];
        let res = sig.invoke(&mut proc, spawn, &args, &out);
        sig.finish(&mut proc, entered.hidden, res, &out);
        proc
    }

    fn worker(
        &self,
        routine: &Routine,
        literals: &mut Vec<(String, Vec<u8>)>,
        cap: &Capture<'_>,
    ) -> Result<Procedure, Fault> {
        let payload = unwrap(&routine.output);
        let name = format!("{}$entry", routine.symbol);
        let mut proc = Procedure::new(&name);
        let base = proc.value();
        proc.parameters.push(Parameter::Register {
            dst: base,
            index: 0,
        });
        let fp = payload.footprint(&self.unit.catalog);
        let out =
            (fp.size > 0).then(|| proc.reserve(fp.size.max(8) as u32, fp.align.max(8) as u32));
        let finish = proc.label();
        let mut func = Function::new(self.unit, literals, proc, payload.clone());
        func.capture = Some(base);
        func.asyncs = Some(Async {
            cleanups: cap.cleanups.to_vec(),
            output: out,
            exit: finish,
        });
        for (pname, kind, off) in cap.params {
            match cap.cleanups.iter().find(|c| &c.name == pname).cloned() {
                Some(clean) => {
                    func.toggle(clean.bit, true);
                    func.scopes[0].push(Local {
                        name: pname.clone(),
                        kind: kind.clone(),
                        storage: Storage::Heap {
                            offset: clean.offset,
                            bit: clean.bit,
                        },
                    });
                }
                None => {
                    let src = Signature::displace(&mut func.proc, base, *off);
                    let val = func.sig().load(&mut func.proc, src, kind);
                    func.bind(pname, kind, val);
                }
            }
        }
        func.body(&routine.body)?;
        func.proc.emit(Step::Mark(finish));
        epilogue(&mut func, base, cap.total, &payload);
        Ok(func.proc)
    }

    fn cleaner(&self, name: &str, cleanups: &[Cleanup], total: i32) -> Procedure {
        let mut proc = Procedure::new(name);
        let sig = Signature::new(&self.unit.catalog, false);
        let env = proc.value();
        proc.parameters
            .push(Parameter::Register { dst: env, index: 0 });
        for item in cleanups.iter().rev() {
            self.guard(&mut proc, env, item);
        }
        let size = Signature::constant(&mut proc, i64::from(total));
        let free = Callee::Direct {
            symbol: FREE.to_owned(),
            external: false,
        };
        let args = [
            (
                Value::Scalar(env),
                Type::Pointer(Box::new(Type::Unsigned(1))),
            ),
            (Value::Scalar(size), Type::Unsigned(8)),
        ];
        let _ = sig.invoke(&mut proc, free, &args, &Type::Void);
        proc.emit(Step::Finish(Ending::Void));
        proc
    }

    fn guard(&self, proc: &mut Procedure, env: u32, item: &Cleanup) {
        let sig = Signature::new(&self.unit.catalog, false);
        let skip = proc.label();
        let mask = sig.fetch(proc, env, 0, 8);
        let flag = Signature::constant(proc, 1i64.wrapping_shl(item.bit));
        let bit = proc.value();
        proc.emit(Step::Binary {
            kind: Binary::Conjunction,
            dst: bit,
            lhs: mask,
            rhs: flag,
        });
        proc.emit(Step::Branch {
            cond: bit,
            truth: false,
            target: skip,
        });
        let clear = Signature::constant(proc, !1i64.wrapping_shl(item.bit));
        let next = proc.value();
        proc.emit(Step::Binary {
            kind: Binary::Conjunction,
            dst: next,
            lhs: mask,
            rhs: clear,
        });
        proc.emit(Step::Store {
            base: env,
            offset: 0,
            src: next,
            width: Width::Quad,
        });
        let addr = Signature::displace(proc, env, item.offset);
        let external = self.unit.imports.iter().any(|i| i.symbol == item.symbol);
        let callee = Callee::Direct {
            symbol: item.symbol.clone(),
            external,
        };
        let args = [(
            Value::Scalar(addr),
            Type::Pointer(Box::new(Type::Unsigned(1))),
        )];
        let _ = sig.invoke(proc, callee, &args, &Type::Void);
        proc.emit(Step::Mark(skip));
    }
}
