use super::abi::{Signature, Value};
use super::{Function, Loop, Storage};
use crate::Fault;
use crate::encode::{Condition, Label, Width};
use crate::machine::linear::{Binary, Callee, Slot, Step};
use simplex_hir::{Case, Expression, Statement, Type};

impl Function<'_, '_> {
    pub fn block(&mut self, body: &[Statement], tail: Option<&Expression>) -> Result<Value, Fault> {
        self.scopes.push(Vec::new());
        for stmt in body {
            self.statement(stmt)?;
        }
        let val = match tail {
            Some(expr) => self.expression(expr)?,
            None => Value::Void,
        };
        self.scopes.pop();
        Ok(val)
    }

    pub fn statement(&mut self, stmt: &Statement) -> Result<(), Fault> {
        match stmt {
            Statement::Bind {
                name, kind, value, ..
            } => {
                let val = self.expression(value)?;
                self.bind(name, kind, val);
            }
            Statement::Assign { name, value } => {
                let val = self.expression(value)?;
                let kind = self.lookup(name)?.kind.clone();
                let ptr = self.address(name)?;
                self.sig().store(&mut self.proc, ptr, val, &kind);
            }
            Statement::Store { target, value } => {
                let kind = self.classify(target);
                let ptr = self.place(target)?;
                let val = self.expression(value)?;
                self.sig().store(&mut self.proc, ptr, val, &kind);
            }
            Statement::Drop {
                name,
                symbol,
                borrowed,
            } => self.drop(name, symbol, *borrowed)?,
            Statement::Break(value) => self.escape(value.as_ref())?,
            Statement::Continue => self.again()?,
            Statement::Evaluate(expr) => {
                let _ = self.expression(expr)?;
            }
            Statement::Result(expr) => {
                let val = self.expression(expr)?;
                self.complete(val);
                self.unreachable();
            }
        }
        Ok(())
    }

    pub(crate) fn again(&mut self) -> Result<(), Fault> {
        let Some(top) = self.loops.last() else {
            return Err(Fault::Unimplemented("continue outside loop".to_owned()));
        };
        self.proc.emit(Step::Jump(top.head));
        self.unreachable();
        Ok(())
    }

    fn unreachable(&mut self) {
        let dead = self.proc.label();
        self.proc.emit(Step::Mark(dead));
    }

    fn drop(&mut self, name: &str, symbol: &str, borrowed: bool) -> Result<(), Fault> {
        let local = self.lookup(name)?.clone();
        if let Storage::Heap { bit, .. } = local.storage {
            self.toggle(bit, false);
        }
        let addr = self.address(name)?;
        let arg = match (&local.kind, borrowed) {
            (Type::Pointer(_), _) => (
                Value::Scalar(self.sig().fetch(&mut self.proc, addr, 0, 8)),
                local.kind.clone(),
            ),
            (_, true) => (
                Value::Scalar(addr),
                Type::Pointer(Box::new(local.kind.clone())),
            ),
            (_, false) => (
                self.sig().load(&mut self.proc, addr, &local.kind),
                local.kind,
            ),
        };
        let resolved = self.resolve(symbol);
        let callee = Callee::Direct {
            symbol: resolved.symbol,
            external: resolved.external,
        };
        let _ = self
            .sig()
            .invoke(&mut self.proc, callee, &[arg], &resolved.output);
        Ok(())
    }

    fn escape(&mut self, value: Option<&Expression>) -> Result<(), Fault> {
        let val = match value {
            Some(expr) => Some(self.expression(expr)?),
            None => None,
        };
        let Some(top) = self.loops.last().cloned() else {
            return Err(Fault::Unimplemented("break outside loop".to_owned()));
        };
        if let (Some(v), Some((slot, kind))) = (val, top.result) {
            let dst = Signature::base(&mut self.proc, slot, 0);
            self.sig().store(&mut self.proc, dst, v, &kind);
        }
        self.proc.emit(Step::Jump(top.exit));
        self.unreachable();
        Ok(())
    }

    pub fn conditional(
        &mut self,
        condition: &Expression,
        arms: (&Expression, &Expression),
        kind: &Type,
    ) -> Result<Value, Fault> {
        let out = self.temp(kind);
        let cond = self.scalar(condition)?;
        let other = self.proc.label();
        let done = self.proc.label();
        self.proc.emit(Step::Branch {
            cond,
            truth: false,
            target: other,
        });
        let first = self.expression(arms.0)?;
        self.save(out, first, kind);
        self.proc.emit(Step::Jump(done));
        self.proc.emit(Step::Mark(other));
        let second = self.expression(arms.1)?;
        self.save(out, second, kind);
        self.proc.emit(Step::Mark(done));
        Ok(self.restore(out, kind))
    }

    pub fn select(
        &mut self,
        subject: &Expression,
        cases: &[Case],
        kind: &Type,
    ) -> Result<Value, Fault> {
        let domain = self.classify(subject);
        let out = self.temp(kind);
        let done = self.proc.label();
        let (disc, base) = match &domain {
            Type::Choice(cname) => {
                let val = self.expression(subject)?;
                let ptr = self.sig().materialize(&mut self.proc, val, &domain);
                let bytes = self
                    .unit
                    .catalog
                    .choices
                    .get(cname)
                    .map_or(1, |c| u64::from(c.discriminant.max(1)));
                (self.sig().fetch(&mut self.proc, ptr, 0, bytes), Some(ptr))
            }
            _ => (self.scalar(subject)?, None),
        };
        for (idx, arm) in cases.iter().enumerate() {
            let last = idx + 1 == cases.len();
            self.arm((disc, base), arm, (out, done, last), kind)?;
        }
        self.proc.emit(Step::Mark(done));
        Ok(self.restore(out, kind))
    }

    fn arm(
        &mut self,
        subj: (u32, Option<u32>),
        case: &Case,
        flow: (Option<Slot>, Label, bool),
        kind: &Type,
    ) -> Result<(), Fault> {
        let (out, done, last) = flow;
        let next = self.proc.label();
        let enter = self.proc.label();
        if last || case.tags.is_empty() {
            self.proc.emit(Step::Jump(enter));
        } else {
            for &tag in &case.tags {
                let target = Signature::constant(&mut self.proc, tag as i64);
                let hit = self.proc.value();
                self.proc.emit(Step::Binary {
                    kind: Binary::Compare(Condition::Equal),
                    dst: hit,
                    lhs: subj.0,
                    rhs: target,
                });
                self.proc.emit(Step::Branch {
                    cond: hit,
                    truth: true,
                    target: enter,
                });
            }
            self.proc.emit(Step::Jump(next));
        }
        self.proc.emit(Step::Mark(enter));
        self.scopes.push(Vec::new());
        if let Some(ptr) = subj.1 {
            for (bname, field) in &case.bindings {
                let addr = Signature::displace(&mut self.proc, ptr, field.offset as i32);
                let val = self.sig().load(&mut self.proc, addr, &field.kind);
                self.bind(bname, &field.kind, val);
            }
        }
        let res = self.expression(&case.body)?;
        self.save(out, res, kind);
        self.scopes.pop();
        self.proc.emit(Step::Jump(done));
        self.proc.emit(Step::Mark(next));
        Ok(())
    }

    pub fn repeat(&mut self, body: &[Statement], kind: &Type) -> Result<Value, Fault> {
        let out = self.temp(kind);
        let head = self.proc.label();
        let exit = self.proc.label();
        self.loops.push(Loop {
            head,
            exit,
            result: out.map(|s| (s, kind.clone())),
        });
        self.proc.emit(Step::Mark(head));
        self.scopes.push(Vec::new());
        for stmt in body {
            self.statement(stmt)?;
        }
        self.scopes.pop();
        self.proc.emit(Step::Jump(head));
        self.proc.emit(Step::Mark(exit));
        self.loops.pop();
        Ok(self.restore(out, kind))
    }

    fn temp(&mut self, kind: &Type) -> Option<Slot> {
        let fp = self.sig().footprint(kind);
        (fp.size > 0).then(|| {
            self.proc
                .reserve(fp.size.max(8) as u32, fp.align.max(8) as u32)
        })
    }

    fn save(&mut self, slot: Option<Slot>, val: Value, kind: &Type) {
        if let Some(s) = slot {
            let dst = Signature::base(&mut self.proc, s, 0);
            self.sig().store(&mut self.proc, dst, val, kind);
        }
    }

    fn restore(&mut self, slot: Option<Slot>, kind: &Type) -> Value {
        match slot {
            Some(s) => {
                let src = Signature::base(&mut self.proc, s, 0);
                self.sig().load(&mut self.proc, src, kind)
            }
            None => Value::Void,
        }
    }

    pub(crate) fn toggle(&mut self, bit: u32, active: bool) {
        let Some(env) = self.capture else {
            return;
        };
        let mask = self.sig().fetch(&mut self.proc, env, 0, 8);
        let flag = 1i64.wrapping_shl(bit);
        let (kind, imm) = match active {
            true => (Binary::Disjunction, flag),
            false => (Binary::Conjunction, !flag),
        };
        let rhs = Signature::constant(&mut self.proc, imm);
        let updated = self.proc.value();
        self.proc.emit(Step::Binary {
            kind,
            dst: updated,
            lhs: mask,
            rhs,
        });
        self.proc.emit(Step::Store {
            base: env,
            offset: 0,
            src: updated,
            width: Width::Quad,
        });
    }
}
