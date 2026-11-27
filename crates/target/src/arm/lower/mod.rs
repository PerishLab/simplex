mod atomic;
mod call;
mod cast;
mod flow;
mod invoke;
mod scan;
mod select;
mod shuffle;
mod survey;

pub use call::footprint;

use super::{Encoder, Label};
use crate::Fault;
use crate::allocate::{Plan, assign};
use crate::machine::{Bank, Location};
use simplex_ir::{Function, Type};
use simplex_object::Relocation;
use std::collections::BTreeMap;

pub trait Resolver {
    fn symbol(&self, name: &str) -> (String, bool);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    Empty,
    Single(u32),
    Pair(u32, u32),
    Slot(usize),
}

#[derive(Clone, Copy)]
pub struct Local {
    pub size: u32,
    pub align: u32,
    pub offset: u32,
}

pub struct Context<'a> {
    pub function: &'a Function,
    pub resolver: &'a dyn Resolver,
    pub encoder: Encoder,
    pub kinds: Vec<Type>,
    pub bindings: Vec<Binding>,
    pub locals: Vec<Local>,
    pub explicit: BTreeMap<(usize, usize), usize>,
    pub copies: BTreeMap<(usize, usize, usize), usize>,
    pub labels: BTreeMap<u32, Label>,
    pub plan: Plan,
    pub indirect: Option<u32>,
    pub spills: u32,
    pub general: (u32, Vec<u8>),
    pub float: (u32, Vec<u8>),
    pub total: u32,
}

pub fn generate(
    function: &Function,
    resolver: &dyn Resolver,
) -> Result<(Vec<u8>, Vec<Relocation>), Fault> {
    if function.asynchronous {
        return Err(Fault::Unsupported(String::from(
            "unprepared async function",
        )));
    }
    let mut planner = survey::Planner::new(function);
    planner.populate()?;
    let spans = planner.spans();
    let bank = Bank {
        general: vec![9, 10, 11, 12, 13],
        preserved: vec![19, 20, 21, 22, 23, 24, 25, 26, 27, 28],
        float: vec![8, 9, 10, 11, 12, 13, 14, 15],
    };
    let plan = assign(&bank, &spans);
    let mut context = planner.assemble(resolver, plan, collect);
    context.prologue();
    for (index, block) in function.blocks.iter().enumerate() {
        context.encoder.bind(context.labels[&block.id]);
        for (position, item) in block.body.iter().enumerate() {
            context.instruction((index, position), item)?;
        }
        context.terminator(&block.exit)?;
    }
    context.encoder.finish()
}

fn collect(plan: &Plan) -> (Vec<u8>, Vec<u8>) {
    let mut general = Vec::new();
    let mut float = Vec::new();
    for location in plan.locations.values() {
        match *location {
            Location::General(reg) if (19..=28).contains(&reg) && !general.contains(&reg) => {
                general.push(reg);
            }
            Location::Float(reg) if (8..=15).contains(&reg) && !float.contains(&reg) => {
                float.push(reg);
            }
            _ => {}
        }
    }
    general.sort_unstable();
    float.sort_unstable();
    (general, float)
}
