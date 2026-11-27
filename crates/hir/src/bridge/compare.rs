use super::builder::Builder;
use super::types::{BOOL, BYTE, WIDE};
use crate::tree::Expression;
use simplex_ir::{Binary, Operation, Target, Terminator};

impl Builder<'_, '_> {
    pub fn aggregates(
        &mut self,
        operator: &str,
        pair: (&Expression, &Expression),
        domain: &crate::Type,
    ) -> u32 {
        let first = self.place(pair.0);
        let second = self.place(pair.1);
        let bytes = domain.footprint(&self.unit.catalog).size;
        let total = self.integer(bytes as i128, WIDE);
        self.scan((first, second, total), operator == "!=")
    }

    pub fn slices(
        &mut self,
        operator: &str,
        pair: (&Expression, &Expression),
        elem: &crate::Type,
    ) -> u32 {
        let (lp, ll) = self.sequence(pair.0);
        let (rp, rl) = self.sequence(pair.1);
        let unequal = operator == "!=";
        let fail = self.integer(i128::from(unequal), BOOL);
        let same = self.emit(
            BOOL,
            Operation::Binary {
                op: Binary::Eq,
                left: ll,
                right: rl,
                kind: WIDE,
            },
        );
        let body = self.block(Vec::new());
        let result = self.fresh();
        let done = self.block(vec![(result, BOOL)]);
        self.terminate(Terminator::Branch {
            condition: same,
            truthy: Target {
                block: body,
                arguments: Vec::new(),
            },
            falsy: Target {
                block: done,
                arguments: vec![fail],
            },
        });
        self.activate(body, true);
        let stride = elem.footprint(&self.unit.catalog).size.max(1) as i128;
        let scale = self.integer(stride, WIDE);
        let total = self.emit(
            WIDE,
            Operation::Binary {
                op: Binary::Mul,
                left: ll,
                right: scale,
                kind: WIDE,
            },
        );
        let checked = self.scan((lp, rp, total), unequal);
        self.terminate(Terminator::Jump(Target {
            block: done,
            arguments: vec![checked],
        }));
        self.activate(done, true);
        result
    }

    fn scan(&mut self, span: (u32, u32, u32), unequal: bool) -> u32 {
        let start = self.integer(0, WIDE);
        let position = self.fresh();
        let head = self.block(vec![(position, WIDE)]);
        let step = self.block(Vec::new());
        let result = self.fresh();
        let done = self.block(vec![(result, BOOL)]);
        self.terminate(Terminator::Jump(Target {
            block: head,
            arguments: vec![start],
        }));
        self.activate(head, true);
        let pass = self.integer(i128::from(!unequal), BOOL);
        let more = self.emit(
            BOOL,
            Operation::Binary {
                op: Binary::Lt,
                left: position,
                right: span.2,
                kind: WIDE,
            },
        );
        self.terminate(Terminator::Branch {
            condition: more,
            truthy: Target {
                block: step,
                arguments: Vec::new(),
            },
            falsy: Target {
                block: done,
                arguments: vec![pass],
            },
        });
        self.activate(step, true);
        let fail = self.integer(i128::from(unequal), BOOL);
        let left = self.byte(span.0, position);
        let right = self.byte(span.1, position);
        let matched = self.emit(
            BOOL,
            Operation::Binary {
                op: Binary::Eq,
                left,
                right,
                kind: BYTE,
            },
        );
        let one = self.integer(1, WIDE);
        let next = self.emit(
            WIDE,
            Operation::Binary {
                op: Binary::Add,
                left: position,
                right: one,
                kind: WIDE,
            },
        );
        self.terminate(Terminator::Branch {
            condition: matched,
            truthy: Target {
                block: head,
                arguments: vec![next],
            },
            falsy: Target {
                block: done,
                arguments: vec![fail],
            },
        });
        self.activate(done, true);
        result
    }

    fn byte(&mut self, base: u32, position: u32) -> u32 {
        let address = self.advance(base, position);
        self.emit(
            BYTE,
            Operation::Load {
                address,
                kind: BYTE,
                volatile: false,
            },
        )
    }
}
