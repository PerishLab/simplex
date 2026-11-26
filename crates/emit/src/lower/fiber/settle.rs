use crate::Fault;
use crate::encode::{Condition, Width};
use crate::lower::Function;
use crate::lower::abi::{Signature, Value};
use crate::machine::linear::{Binary, Callee, Ending, Invocation, Step};
use crate::plan::FREE;
use simplex_hir::{Case, Expression, Passing, Statement, Type};
use simplex_runtime::Hook;

pub struct Tracked {
    pub order: usize,
    pub name: String,
    pub symbol: String,
    pub kind: Type,
}

pub fn collect(inputs: &[(String, Type)], stmts: &[Statement]) -> Vec<(String, String, Type)> {
    let mut locals: Vec<(usize, String, Type)> = inputs
        .iter()
        .enumerate()
        .map(|(i, (n, k))| (i, n.clone(), k.clone()))
        .collect();
    let mut found: Vec<Tracked> = Vec::new();
    scan(stmts, &mut locals, &mut found);
    found.sort_by_key(|item| item.order);
    found
        .into_iter()
        .map(|t| (t.name, t.symbol, t.kind))
        .collect()
}

fn scan(stmts: &[Statement], locals: &mut Vec<(usize, String, Type)>, out: &mut Vec<Tracked>) {
    for stmt in stmts {
        match stmt {
            Statement::Bind {
                name, kind, value, ..
            } => {
                nested(value, locals, out);
                let idx = locals.len();
                locals.push((idx, name.clone(), kind.clone()));
            }
            Statement::Drop { name, symbol, .. } => record(name, symbol, locals, out),
            Statement::Assign { value, .. }
            | Statement::Evaluate(value)
            | Statement::Result(value) => nested(value, locals, out),
            Statement::Store { target, value } => {
                nested(target, locals, out);
                nested(value, locals, out);
            }
            Statement::Break(Some(expr)) => nested(expr, locals, out),
            _ => {}
        }
    }
}

fn record(name: &str, symbol: &str, locals: &[(usize, String, Type)], out: &mut Vec<Tracked>) {
    if out.iter().any(|d| d.name == name) {
        return;
    }
    if let Some((order, _, kind)) = locals.iter().rev().find(|l| l.1 == name)
        && !matches!(kind, Type::Pointer(_))
    {
        out.push(Tracked {
            order: *order,
            name: name.to_owned(),
            symbol: symbol.to_owned(),
            kind: kind.clone(),
        });
    }
}

fn nested(expr: &Expression, locals: &mut Vec<(usize, String, Type)>, out: &mut Vec<Tracked>) {
    match expr {
        Expression::Block { body, tail, .. } => {
            scan(body, locals, out);
            if let Some(t) = tail {
                nested(t, locals, out);
            }
        }
        Expression::Branch {
            condition,
            then,
            otherwise,
            ..
        } => {
            nested(condition, locals, out);
            nested(then, locals, out);
            nested(otherwise, locals, out);
        }
        Expression::Switch { subject, cases, .. } => {
            nested(subject, locals, out);
            for Case { body, .. } in cases {
                nested(body, locals, out);
            }
        }
        Expression::Loop { body, .. } => scan(body, locals, out),
        _ => {}
    }
}

pub fn epilogue(func: &mut Function<'_, '_>, env: u32, total: i32, payload: &Type) {
    let slot = func.asyncs.as_ref().and_then(|a| a.output);
    let fp = func.sig().footprint(payload);
    let ending = match (fp.passing, slot) {
        (Passing::Void, _) => {
            release(func, env, total);
            Ending::Single(Signature::constant(&mut func.proc, 0))
        }
        (Passing::Register, Some(s)) => {
            let addr = Signature::base(&mut func.proc, s, 0);
            let Value::Scalar(v) = func.sig().load(&mut func.proc, addr, payload) else {
                return;
            };
            release(func, env, total);
            Ending::Single(v)
        }
        (Passing::Pair, Some(s)) => {
            let addr = Signature::base(&mut func.proc, s, 0);
            let Value::Pair(a, b) = func.sig().load(&mut func.proc, addr, payload) else {
                return;
            };
            release(func, env, total);
            Ending::Pair(a, b)
        }
        (Passing::Indirect, Some(s)) => {
            let src = Signature::base(&mut func.proc, s, 0);
            let size = Signature::constant(&mut func.proc, fp.size as i64);
            let heap = func.allocate(size);
            func.proc.emit(Step::Copy {
                dst: heap,
                src,
                bytes: fp.size as u32,
            });
            release(func, env, total);
            Ending::Single(heap)
        }
        _ => {
            release(func, env, total);
            Ending::Single(Signature::constant(&mut func.proc, 0))
        }
    };
    func.proc.emit(Step::Finish(ending));
}

pub fn release(func: &mut Function<'_, '_>, ptr: u32, bytes: i32) {
    let size = Signature::constant(&mut func.proc, i64::from(bytes));
    let callee = Callee::Direct {
        symbol: FREE.to_owned(),
        external: false,
    };
    let args = [
        (
            Value::Scalar(ptr),
            Type::Pointer(Box::new(Type::Unsigned(1))),
        ),
        (Value::Scalar(size), Type::Unsigned(8)),
    ];
    let _ = func
        .sig()
        .invoke(&mut func.proc, callee, &args, &Type::Void);
}

impl Function<'_, '_> {
    pub fn settle(&mut self, hook: Hook, args: Vec<u32>, kind: &Type) -> Value {
        let first = self.proc.value();
        let second = self.proc.value();
        self.proc.emit(Step::Invoke(Invocation {
            callee: Callee::Direct {
                symbol: hook.symbol().to_owned(),
                external: false,
            },
            registers: args,
            floats: Vec::new(),
            stack: Vec::new(),
            space: 0,
            first: Some(first),
            second: Some(second),
            media: None,
        }));
        self.unpack(first, second, kind)
    }

    pub(crate) fn unpack(&mut self, first: u32, second: u32, kind: &Type) -> Value {
        let fp = self.sig().footprint(kind);
        match fp.passing {
            Passing::Void => Value::Void,
            Passing::Register => Value::Scalar(Signature::normalize(&mut self.proc, first, kind)),
            Passing::Pair => Value::Pair(first, second),
            Passing::Indirect => {
                let slot = self.proc.reserve(fp.size as u32, fp.align as u32);
                let dst = Signature::base(&mut self.proc, slot, 0);
                self.proc.emit(Step::Copy {
                    dst,
                    src: first,
                    bytes: fp.size as u32,
                });
                release(self, first, fp.size as i32);
                Value::Address(dst)
            }
        }
    }

    pub fn gather(&mut self, fibers: &Expression, element: &Type) -> Result<Value, Fault> {
        let (fptr, count) = self.sequence(fibers)?;
        let sixteen = Signature::constant(&mut self.proc, 16);
        let rawbytes = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Mul,
            dst: rawbytes,
            lhs: count,
            rhs: sixteen,
        });
        let raw = self.allocate(rawbytes);
        let _ = self.settle(Hook::All, vec![fptr, count, raw], &Type::Unsigned(8));
        let stride = self.sig().footprint(element).size.max(1) as i64;
        let factor = Signature::constant(&mut self.proc, stride);
        let outbytes = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Mul,
            dst: outbytes,
            lhs: count,
            rhs: factor,
        });
        let out = self.allocate(outbytes);
        self.repack((raw, out, count), element);
        release(self, raw, 4096);
        Ok(Value::Pair(out, count))
    }

    fn repack(&mut self, bufs: (u32, u32, u32), element: &Type) {
        let idx = self.proc.reserve(8, 8);
        let cursor = Signature::base(&mut self.proc, idx, 0);
        self.proc.emit(Step::Zero {
            dst: cursor,
            bytes: 8,
        });
        let head = self.proc.label();
        let body = self.proc.label();
        let done = self.proc.label();
        self.proc.emit(Step::Mark(head));
        let pos = self.sig().fetch(&mut self.proc, cursor, 0, 8);
        let more = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Compare(Condition::Below),
            dst: more,
            lhs: pos,
            rhs: bufs.2,
        });
        self.proc.emit(Step::Branch {
            cond: more,
            truth: true,
            target: body,
        });
        self.proc.emit(Step::Jump(done));
        self.proc.emit(Step::Mark(body));
        let sixteen = Signature::constant(&mut self.proc, 16);
        let soff = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Mul,
            dst: soff,
            lhs: pos,
            rhs: sixteen,
        });
        let saddr = self.advance(bufs.0, soff);
        let r0 = self.sig().fetch(&mut self.proc, saddr, 0, 8);
        let r1 = self.sig().fetch(&mut self.proc, saddr, 8, 8);
        let val = self.unpack(r0, r1, element);
        let stride = self.sig().footprint(element).size.max(1) as i64;
        let dmul = Signature::constant(&mut self.proc, stride);
        let doff = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Mul,
            dst: doff,
            lhs: pos,
            rhs: dmul,
        });
        let daddr = self.advance(bufs.1, doff);
        self.sig().store(&mut self.proc, daddr, val, element);
        let one = Signature::constant(&mut self.proc, 1);
        let next = self.advance(pos, one);
        self.proc.emit(Step::Store {
            base: cursor,
            offset: 0,
            src: next,
            width: Width::Quad,
        });
        self.proc.emit(Step::Jump(head));
        self.proc.emit(Step::Mark(done));
    }
}
