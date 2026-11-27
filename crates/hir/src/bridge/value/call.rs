use crate::bridge::builder::{Builder, Resolved};
use crate::bridge::types::convert;
use crate::tree::Expression;
use simplex_ir::Operation;

impl Builder<'_, '_> {
    pub fn direct(&mut self, symbol: &str, arguments: &[Expression], kind: &crate::Type) -> u32 {
        let resolved = self.resolve(symbol);
        let (values, inputs) = self.stage(arguments, &resolved.inputs);
        if resolved.asynchronous {
            let result = convert(&resolved.payload, &self.unit.catalog);
            return self.emit(
                simplex_ir::Type::Pointer,
                Operation::Spawn {
                    symbol: resolved.symbol,
                    arguments: values,
                    inputs,
                    result,
                },
            );
        }
        let actual = match *kind == crate::Type::Void {
            true => convert(&resolved.output, &self.unit.catalog),
            false => convert(kind, &self.unit.catalog),
        };
        if resolved.missing {
            self.vault
                .unresolved
                .entry(resolved.symbol.clone())
                .or_insert_with(|| (inputs.clone(), actual.clone()));
        }
        self.emit(
            actual.clone(),
            Operation::Call {
                symbol: resolved.symbol,
                arguments: values,
                inputs,
                kind: actual,
            },
        )
    }

    pub fn dispatch(
        &mut self,
        receiver: (&Expression, usize),
        arguments: &[Expression],
        kind: &crate::Type,
    ) -> u32 {
        let pair = self.expression(receiver.0);
        let data = self.emit(
            simplex_ir::Type::Pointer,
            Operation::Low {
                pair,
                kind: simplex_ir::Type::Pointer,
            },
        );
        let table = self.emit(
            simplex_ir::Type::Pointer,
            Operation::High {
                pair,
                kind: simplex_ir::Type::Pointer,
            },
        );
        let entry = self.displace(table, (receiver.1 * 8) as i64);
        let callee = self.emit(
            simplex_ir::Type::Pointer,
            Operation::Load {
                address: entry,
                kind: simplex_ir::Type::Pointer,
                volatile: false,
            },
        );
        let (mut values, mut inputs) = (vec![data], vec![simplex_ir::Type::Pointer]);
        let (rest, types) = self.stage(arguments, &[]);
        values.extend(rest);
        inputs.extend(types);
        let lowered = convert(kind, &self.unit.catalog);
        self.emit(
            lowered.clone(),
            Operation::Indirect {
                callee,
                arguments: values,
                inputs,
                kind: lowered,
            },
        )
    }

    pub fn indirect(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        kind: &crate::Type,
    ) -> u32 {
        let target = self.expression(callee);
        let expected = match self.classify(callee) {
            crate::Type::Function { inputs, .. } => inputs,
            _ => Vec::new(),
        };
        let (values, inputs) = self.stage(arguments, &expected);
        let lowered = convert(kind, &self.unit.catalog);
        self.emit(
            lowered.clone(),
            Operation::Indirect {
                callee: target,
                arguments: values,
                inputs,
                kind: lowered,
            },
        )
    }

    fn stage(
        &mut self,
        arguments: &[Expression],
        expected: &[crate::Type],
    ) -> (Vec<u32>, Vec<simplex_ir::Type>) {
        let mut values = Vec::with_capacity(arguments.len());
        let mut inputs = Vec::with_capacity(arguments.len());
        for (index, expr) in arguments.iter().enumerate() {
            let actual = self.classify(expr);
            let raw = self.expression(expr);
            let kind = expected
                .get(index)
                .cloned()
                .unwrap_or_else(|| actual.clone());
            let value = self.recast(raw, &actual, &kind);
            let lowered = convert(&kind, &self.unit.catalog);
            if lowered != simplex_ir::Type::Void {
                values.push(value);
                inputs.push(lowered);
            }
        }
        (values, inputs)
    }

    pub fn resolve(&self, symbol: &str) -> Resolved {
        if let Some(routine) = self.unit.routines.iter().find(|item| item.symbol == symbol) {
            let output = match routine.asynchronous {
                true => crate::Type::Fiber(Box::new(routine.output.clone())),
                false => routine.output.clone(),
            };
            return Resolved {
                symbol: routine.symbol.clone(),
                inputs: routine
                    .inputs
                    .iter()
                    .map(|(_, kind)| kind.clone())
                    .collect(),
                output,
                payload: routine.output.clone(),
                asynchronous: routine.asynchronous,
                missing: false,
            };
        }
        if let Some(imported) = self
            .unit
            .imports
            .iter()
            .find(|item| item.symbol == symbol || item.local == symbol)
        {
            return Resolved {
                symbol: imported.symbol.clone(),
                inputs: imported.inputs.clone(),
                output: imported.output.clone(),
                payload: imported.output.clone(),
                asynchronous: false,
                missing: false,
            };
        }
        Resolved {
            symbol: symbol.to_owned(),
            inputs: Vec::new(),
            output: crate::Type::Void,
            payload: crate::Type::Void,
            asynchronous: false,
            missing: true,
        }
    }
}
