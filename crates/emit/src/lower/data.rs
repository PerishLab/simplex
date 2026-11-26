use super::Function;
use super::abi::{Signature, Value, Write};
use super::kind::infer;
use crate::Fault;
use crate::encode::Width;
use crate::machine::linear::{Access, Binary, Callee, Step};
use crate::plan::ALLOC;
use simplex_hir::{Expression, Type};

impl Function<'_, '_> {
    pub fn record(
        &mut self,
        name: &str,
        fields: &[(String, Expression)],
        kind: &Type,
    ) -> Result<Value, Fault> {
        let fp = self.sig().footprint(kind);
        let span = fp.size.max(8) as u32;
        let slot = self.proc.reserve(span, fp.align.max(8) as u32);
        let base = Signature::base(&mut self.proc, slot, 0);
        self.proc.emit(Step::Zero {
            dst: base,
            bytes: span,
        });
        let catalog = &self.unit.catalog;
        let Some(layout) = catalog.records.get(name) else {
            return Ok(self.sig().load(&mut self.proc, base, kind));
        };
        let schema = layout.fields.clone();
        for (fname, expr) in fields {
            if let Some(meta) = schema.iter().find(|f| &f.name == fname) {
                let val = self.expression(expr)?;
                let dst = Signature::displace(&mut self.proc, base, meta.offset as i32);
                self.sig().store(&mut self.proc, dst, val, &meta.kind);
            }
        }
        Ok(self.sig().load(&mut self.proc, base, kind))
    }

    pub fn variant(
        &mut self,
        choice: (&str, &str, u64),
        payloads: &[Expression],
        kind: &Type,
    ) -> Result<Value, Fault> {
        let (cname, aname, tag) = choice;
        let fp = self.sig().footprint(kind);
        let span = fp.size.max(8) as u32;
        let slot = self.proc.reserve(span, fp.align.max(8) as u32);
        let base = Signature::base(&mut self.proc, slot, 0);
        self.proc.emit(Step::Zero {
            dst: base,
            bytes: span,
        });
        let catalog = &self.unit.catalog;
        let disc = catalog
            .choices
            .get(cname)
            .map_or(1, |c| u64::from(c.discriminant.max(1)));
        let mark = Signature::constant(&mut self.proc, tag as i64);
        self.sig().write(
            &mut self.proc,
            Write {
                ptr: base,
                offset: 0,
                bytes: disc,
            },
            mark,
        );
        let schema = catalog
            .choices
            .get(cname)
            .and_then(|c| c.arms.iter().find(|a| a.name == aname || a.tag == tag))
            .map(|a| a.payloads.clone())
            .unwrap_or_default();
        for (idx, expr) in payloads.iter().enumerate() {
            let fkind = schema
                .get(idx)
                .map_or_else(|| infer(expr), |f| f.kind.clone());
            let off = schema
                .get(idx)
                .map_or(8 + (idx * 8) as i32, |f| f.offset as i32);
            let val = self.expression(expr)?;
            let dst = Signature::displace(&mut self.proc, base, off);
            self.sig().store(&mut self.proc, dst, val, &fkind);
        }
        Ok(self.sig().load(&mut self.proc, base, kind))
    }

    pub fn list(&mut self, items: &[Expression], element: &Type) -> Result<Value, Fault> {
        let stride = self.sig().footprint(element).size.max(1);
        let bytes = (items.len() as u64 * stride).max(8);
        let size = Signature::constant(&mut self.proc, bytes as i64);
        let ptr = self.allocate(size);
        for (idx, item) in items.iter().enumerate() {
            let val = self.expression(item)?;
            let dst = Signature::displace(&mut self.proc, ptr, (idx as u64 * stride) as i32);
            self.sig().store(&mut self.proc, dst, val, element);
        }
        let len = Signature::constant(&mut self.proc, items.len() as i64);
        Ok(Value::Pair(ptr, len))
    }

    pub fn buffer(&mut self, count: &Expression, element: &Type) -> Result<Value, Fault> {
        let len = self.scalar(count)?;
        let stride = self.sig().footprint(element).size.max(1) as i64;
        let factor = Signature::constant(&mut self.proc, stride);
        let bytes = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Mul,
            dst: bytes,
            lhs: len,
            rhs: factor,
        });
        let ptr = self.allocate(bytes);
        Ok(Value::Pair(ptr, len))
    }

    pub(crate) fn allocate(&mut self, bytes: u32) -> u32 {
        let out = Type::Pointer(Box::new(Type::Unsigned(1)));
        let args = [(Value::Scalar(bytes), Type::Unsigned(8))];
        let callee = Callee::Direct {
            symbol: ALLOC.to_owned(),
            external: false,
        };
        match self.sig().invoke(&mut self.proc, callee, &args, &out) {
            Value::Scalar(ptr) => ptr,
            _ => Signature::constant(&mut self.proc, 0),
        }
    }

    pub fn window(
        &mut self,
        base: &Expression,
        bounds: (&Expression, &Expression),
        kind: &Type,
    ) -> Result<Value, Fault> {
        let (data, _) = self.sequence(base)?;
        let start = self.scalar(bounds.0)?;
        let limit = self.scalar(bounds.1)?;
        let elem = match kind {
            Type::Slice(inner) | Type::List(inner) => (**inner).clone(),
            _ => Type::Unsigned(1),
        };
        let stride = self.sig().footprint(&elem).size.max(1) as i64;
        let scale = Signature::constant(&mut self.proc, stride);
        let delta = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Mul,
            dst: delta,
            lhs: start,
            rhs: scale,
        });
        let ptr = self.advance(data, delta);
        let len = self.proc.value();
        self.proc.emit(Step::Binary {
            kind: Binary::Sub,
            dst: len,
            lhs: limit,
            rhs: start,
        });
        Ok(Value::Pair(ptr, len))
    }

    pub fn coerce(&mut self, value: &Expression, table: &str) -> Result<Value, Fault> {
        let data = self.scalar(value)?;
        let defined = self.unit.tables.iter().any(|t| t.symbol == table);
        let vptr = self.proc.value();
        self.proc.emit(Step::Symbol {
            dst: vptr,
            name: table.to_owned(),
            external: !defined,
        });
        Ok(Value::Pair(data, vptr))
    }

    pub fn direct(
        &mut self,
        symbol: &str,
        arguments: &[Expression],
        kind: &Type,
    ) -> Result<Value, Fault> {
        let resolved = self.resolve(symbol);
        let mut args = Vec::with_capacity(arguments.len());
        for (idx, expr) in arguments.iter().enumerate() {
            let val = self.expression(expr)?;
            let expected = resolved
                .inputs
                .get(idx)
                .cloned()
                .unwrap_or_else(|| infer(expr));
            args.push((val, expected));
        }
        let sig = Signature::new(&self.unit.catalog, resolved.foreign);
        let callee = Callee::Direct {
            symbol: resolved.symbol,
            external: resolved.external,
        };
        let actual = match *kind == Type::Void {
            true => resolved.output,
            false => kind.clone(),
        };
        Ok(sig.invoke(&mut self.proc, callee, &args, &actual))
    }

    pub fn dispatch(
        &mut self,
        receiver: (&Expression, usize),
        arguments: &[Expression],
        kind: &Type,
    ) -> Result<Value, Fault> {
        let (recv, slot) = receiver;
        let Value::Pair(data, vptr) = self.expression(recv)? else {
            return Err(Fault::Unimplemented("expected trait pair".to_owned()));
        };
        let target = self.proc.value();
        self.proc.emit(Step::Load {
            dst: target,
            base: vptr,
            offset: (slot * 8) as i32,
            access: Access {
                width: Width::Quad,
                signed: false,
            },
        });
        let mut args = Vec::with_capacity(arguments.len() + 1);
        args.push((
            Value::Scalar(data),
            Type::Pointer(Box::new(Type::Unsigned(1))),
        ));
        for expr in arguments {
            args.push((self.expression(expr)?, infer(expr)));
        }
        Ok(self
            .sig()
            .invoke(&mut self.proc, Callee::Indirect(target), &args, kind))
    }

    pub fn indirect(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        kind: &Type,
    ) -> Result<Value, Fault> {
        let target = self.scalar(callee)?;
        let mut args = Vec::with_capacity(arguments.len());
        for expr in arguments {
            args.push((self.expression(expr)?, infer(expr)));
        }
        Ok(self
            .sig()
            .invoke(&mut self.proc, Callee::Indirect(target), &args, kind))
    }
}
