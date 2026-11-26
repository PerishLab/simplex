mod decl;
mod expr;

use crate::code::{Code, Finding};
use crate::tree::{
    Binding, Block, Branch, Dispatch, Expression, Item, Loop, Outcome, Parameter, Partition,
    Pattern, Program, Spec, Statement,
};

pub fn render(program: &Program) -> String {
    let mut printer = Printer {
        out: String::new(),
        indent: 0,
    };
    printer.items(&program.items);
    if !printer.out.is_empty() {
        printer.out.push('\n');
    }
    printer.out
}

pub fn verify(source: &str, program: &Program) -> Result<String, Finding> {
    let formatted = render(program);
    if source == formatted {
        return Ok(formatted);
    }
    let (line, column) = mismatch(source, &formatted);
    Err(Finding {
        code: Code::Format,
        line,
        column,
    })
}

pub(crate) struct Printer {
    pub(crate) out: String,
    pub(crate) indent: usize,
}

impl Printer {
    pub(crate) fn margin(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    pub(crate) fn items(&mut self, list: &[Item]) {
        for (index, item) in list.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
                if item.blank {
                    self.out.push('\n');
                }
            }
            self.margin();
            self.statement(&item.statement);
        }
    }

    fn statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Bind(binding) => self.binding(binding),
            Statement::Method(method) => self.method(method),
            Statement::Struct(item) => self.structure(item),
            Statement::Enum(item) => self.enumeration(item),
            Statement::Interface(item) => self.contract(item),
            Statement::Alias(item) => self.alias(item),
            Statement::Conform { subject, contract } => self.conform(subject, contract),
            Statement::Assign {
                target,
                operator,
                value,
            } => {
                self.expression(target);
                self.out.push(' ');
                self.out.push_str(operator);
                self.out.push(' ');
                self.expression(value);
            }
            Statement::For { item, source, body } => {
                self.out.push_str("for ");
                self.out.push_str(item);
                self.out.push_str(" in ");
                self.expression(source);
                self.out.push(' ');
                self.block(body);
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.expression(expr),
        }
    }

    fn binding(&mut self, binding: &Binding) {
        if binding.mutable {
            self.out.push_str("var ");
        }
        self.out.push_str(&binding.name);
        if let Some(spec) = &binding.annotation {
            self.out.push_str(": ");
            self.spec(spec);
            self.out.push_str(" = ");
        } else {
            self.out.push_str(" := ");
        }
        self.expression(&binding.value);
    }

    pub(crate) fn spec(&mut self, spec: &Spec) {
        match spec {
            Spec::Named { name, arguments } => {
                self.out.push_str(name);
                if !arguments.is_empty() {
                    self.out.push('<');
                    self.specs(arguments);
                    self.out.push('>');
                }
            }
            Spec::Callable { inputs, output } => {
                self.out.push_str("fn(");
                self.specs(inputs);
                self.out.push(')');
                if let Some(result) = output {
                    self.out.push_str(" -> ");
                    self.spec(result);
                }
            }
        }
    }

    pub(crate) fn specs(&mut self, list: &[Spec]) {
        for (index, item) in list.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            self.spec(item);
        }
    }

    pub(crate) fn parameters(&mut self, inputs: &[Parameter]) {
        self.out.push('(');
        for (index, param) in inputs.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(&param.name);
            if let Some(spec) = &param.annotation {
                self.out.push_str(": ");
                self.spec(spec);
            }
        }
        self.out.push(')');
    }

    pub(crate) fn block(&mut self, block: &Block) {
        if block.items.is_empty() {
            self.out.push_str("{}");
            return;
        }
        if block.compact {
            self.out.push_str("{ ");
            self.statement(&block.items[0].statement);
            self.out.push_str(" }");
            return;
        }
        self.out.push_str("{\n");
        self.indent += 1;
        self.items(&block.items);
        self.indent -= 1;
        self.out.push('\n');
        self.margin();
        self.out.push('}');
    }

    pub(crate) fn repetition(&mut self, kind: &Loop) {
        self.out.push_str("loop ");
        match kind {
            Loop::Block(block) => self.block(block),
            Loop::Switch(dispatch) => self.dispatch(dispatch),
            Loop::Bucket(partition) => self.partition(partition),
        }
    }

    pub(crate) fn dispatch(&mut self, dispatch: &Dispatch) {
        self.out.push_str("switch ");
        self.expression(&dispatch.subject);
        self.out.push(' ');
        self.arms(&dispatch.arms);
    }

    pub(crate) fn partition(&mut self, partition: &Partition) {
        self.out.push_str("bucket ");
        if let Some(subject) = &partition.subject {
            self.expression(subject);
            self.out.push(' ');
        }
        self.arms(&partition.arms);
    }

    fn arms(&mut self, list: &[Branch]) {
        self.out.push_str("{\n");
        self.indent += 1;
        for (index, arm) in list.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
            }
            self.margin();
            self.pattern(&arm.pattern);
            self.out.push_str(" => ");
            self.outcome(&arm.outcome);
        }
        self.indent -= 1;
        self.out.push('\n');
        self.margin();
        self.out.push('}');
    }

    fn pattern(&mut self, pat: &Pattern) {
        match pat {
            Pattern::Wildcard => self.out.push('_'),
            Pattern::Choices(list) => self.choices(list),
            Pattern::Range { start, end } => {
                self.expression(start);
                self.out.push_str("..");
                self.expression(end);
            }
            Pattern::Predicate(expr) => self.expression(expr),
        }
    }

    fn choices(&mut self, list: &[Expression]) {
        for (index, item) in list.iter().enumerate() {
            if index > 0 {
                self.out.push_str(" | ");
            }
            self.expression(item);
        }
    }

    fn outcome(&mut self, out: &Outcome) {
        match out {
            Outcome::Value(expr) => self.expression(expr),
            Outcome::Break(Some(expr)) => {
                self.out.push_str("break ");
                self.expression(expr);
            }
            Outcome::Break(None) => self.out.push_str("break"),
            Outcome::Continue => self.out.push_str("continue"),
            Outcome::Block(block) => self.block(block),
        }
    }
}

fn mismatch(left: &str, right: &str) -> (usize, usize) {
    let (mut line, mut column) = (1usize, 1usize);
    for (a, b) in left.chars().zip(right.chars()) {
        if a != b {
            return (line, column);
        }
        if a == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}
