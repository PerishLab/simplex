use super::{Context, Spot};
use crate::{Fault, Operation, Target, Terminator, Type};
use std::collections::HashSet;

impl Context<'_> {
    pub(super) fn extended(&self, spot: Spot, op: &Operation, out: &Type) -> Result<(), Fault> {
        match op {
            Operation::Acquire { address, kind, .. } => {
                self.require(spot, *address, &Type::Pointer)?;
                self.expect(kind.atomic() && out == kind, "invalid atomic acquire")
            }
            Operation::Release {
                address,
                value,
                kind,
                ..
            } => {
                self.require(spot, *address, &Type::Pointer)?;
                self.require(spot, *value, kind)?;
                self.expect(
                    kind.atomic() && *out == Type::Void,
                    "invalid atomic release",
                )
            }
            Operation::Swap {
                address,
                expected,
                desired,
                kind,
                ..
            } => {
                self.require(spot, *address, &Type::Pointer)?;
                self.require(spot, *expected, kind)?;
                self.require(spot, *desired, kind)?;
                self.expect(kind.atomic() && out == kind, "invalid compare swap")
            }
            Operation::Atomic {
                address,
                operand,
                kind,
                ..
            } => {
                self.require(spot, *address, &Type::Pointer)?;
                self.require(spot, *operand, kind)?;
                self.expect(kind.atomic() && out == kind, "invalid atomic modify")
            }
            Operation::Fence { .. } => self.expect(*out == Type::Void, "fence must return void"),
            Operation::Call {
                symbol,
                arguments,
                inputs,
                kind,
            } => self.direct(spot, (symbol, arguments), (inputs, kind, out)),
            Operation::Indirect {
                callee,
                arguments,
                inputs,
                kind,
            } => {
                self.require(spot, *callee, &Type::Pointer)?;
                self.signature(spot, arguments, inputs)?;
                self.expect(out == kind, "indirect result mismatch")
            }
            Operation::Spawn {
                symbol,
                arguments,
                inputs,
                result,
            } => self.spawn(spot, (symbol, arguments), (inputs, result, out)),
            Operation::Yield => self.expect(
                self.function.asynchronous && *out == Type::Void,
                "yield requires async void",
            ),
            Operation::Await { fiber, kind } => {
                self.require(spot, *fiber, &Type::Pointer)?;
                self.expect(out == kind, "await result mismatch")
            }
            Operation::All {
                fibers,
                count,
                element,
            } => {
                self.require(spot, *fibers, &Type::Pointer)?;
                self.counter(spot, *count)?;
                self.expect(
                    element.valid() && *out == Type::Pair,
                    "invalid all operation",
                )
            }
            Operation::Race {
                fibers,
                count,
                kind,
            } => {
                self.require(spot, *fibers, &Type::Pointer)?;
                self.counter(spot, *count)?;
                self.expect(out == kind, "invalid race operation")
            }
            _ => Ok(()),
        }
    }

    pub fn terminator(&self, spot: Spot, exit: &Terminator) -> Result<(), Fault> {
        match exit {
            Terminator::Jump(edge) => self.target(spot, edge),
            Terminator::Branch {
                condition,
                truthy,
                falsy,
            } => {
                let boolean = Type::Integer {
                    bits: 1,
                    signed: false,
                };
                self.require(spot, *condition, &boolean)?;
                self.target(spot, truthy)?;
                self.target(spot, falsy)
            }
            Terminator::Switch {
                value,
                cases,
                default,
            } => self.switch(spot, *value, (cases, default)),
            Terminator::Return(Some(val)) => {
                self.expect(
                    self.function.output != Type::Void,
                    "void function returned value",
                )?;
                self.require(spot, *val, &self.function.output)
            }
            Terminator::Return(None) => {
                self.expect(self.function.output == Type::Void, "missing return value")
            }
            Terminator::Trap { .. } => Ok(()),
        }
    }

    fn switch(
        &self,
        spot: Spot,
        value: u32,
        arms: (&[(u64, Target)], &Target),
    ) -> Result<(), Fault> {
        let domain = self.lookup(spot, value)?;
        self.expect(
            matches!(domain, Type::Integer { .. }),
            "switch value must be integer",
        )?;
        let mut seen = HashSet::with_capacity(arms.0.len());
        for (tag, edge) in arms.0 {
            self.expect(seen.insert(*tag), "duplicate switch case")?;
            self.target(spot, edge)?;
        }
        self.target(spot, arms.1)
    }

    fn target(&self, spot: Spot, edge: &Target) -> Result<(), Fault> {
        let Some(&dst) = self.tree.map.get(&edge.block) else {
            return Err(self.fail(&format!("unknown target block {}", edge.block)));
        };
        let params = &self.function.blocks[dst].parameters;
        if edge.arguments.len() != params.len() {
            return Err(self.fail(&format!("block {} argument count mismatch", edge.block)));
        }
        for (&arg, (_, expected)) in edge.arguments.iter().zip(params.iter()) {
            self.require(spot, arg, expected)?;
        }
        Ok(())
    }

    fn direct(
        &self,
        spot: Spot,
        call: (&str, &[u32]),
        types: (&[Type], &Type, &Type),
    ) -> Result<(), Fault> {
        let (symbol, arguments) = call;
        let (inputs, kind, out) = types;
        self.expect(!symbol.is_empty() && out == kind, "invalid call target")?;
        self.signature(spot, arguments, inputs)?;
        let Some(entry) = self.catalog.callables.get(symbol) else {
            return Err(self.fail(&format!("unknown call target {symbol}")));
        };
        self.expect(!entry.asynchronous, "async function must use spawn")?;
        self.expect(
            entry.inputs == inputs && &entry.output == kind,
            "call signature mismatch",
        )
    }

    fn spawn(
        &self,
        spot: Spot,
        call: (&str, &[u32]),
        types: (&[Type], &Type, &Type),
    ) -> Result<(), Fault> {
        let (symbol, arguments) = call;
        let (inputs, result, out) = types;
        self.expect(
            !symbol.is_empty() && result.valid() && *out == Type::Pointer,
            "invalid spawn",
        )?;
        self.signature(spot, arguments, inputs)?;
        let Some(entry) = self.catalog.callables.get(symbol) else {
            return Err(self.fail(&format!("unknown spawn target {symbol}")));
        };
        if entry.asynchronous {
            return self.expect(
                entry.inputs == inputs && &entry.output == result,
                "spawn signature mismatch",
            );
        }
        self.expect(
            entry.inputs == inputs && entry.output == Type::Pointer,
            "external spawn signature mismatch",
        )
    }

    fn signature(&self, spot: Spot, arguments: &[u32], inputs: &[Type]) -> Result<(), Fault> {
        self.expect(
            arguments.len() == inputs.len(),
            "call argument count mismatch",
        )?;
        for (&arg, expected) in arguments.iter().zip(inputs.iter()) {
            self.expect(expected.concrete(), "void call input")?;
            self.require(spot, arg, expected)?;
        }
        Ok(())
    }

    fn counter(&self, spot: Spot, value: u32) -> Result<(), Fault> {
        let kind = self.lookup(spot, value)?;
        self.expect(
            matches!(kind, Type::Integer { bits: 64, .. }),
            "fiber count must be 64-bit integer",
        )
    }
}
