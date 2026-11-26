use super::Table;
use crate::code::{Code, Finding};
use crate::tree::{
    Binding, Block, Body, Branch, Dispatch, Item, Loop, Outcome, Partition, Program, Spec,
    Statement,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    Integer,
    Float,
    Scalar(String),
    Nominal(String),
    Generic(String),
    Borrow { mutable: bool, inner: Box<Type> },
    Slice(Box<Type>),
    List(Box<Type>),
    Fiber(Box<Type>),
    Unknown,
}

pub(crate) fn verify(table: &Table, program: &Program) -> Result<(), Finding> {
    let mut scope = Scope {
        table,
        locals: BTreeMap::new(),
        generics: BTreeMap::new(),
    };
    scope.items(&program.items)?;
    Ok(())
}

pub(crate) struct Scope<'a> {
    pub(crate) table: &'a Table,
    pub(crate) locals: BTreeMap<String, Type>,
    pub(crate) generics: BTreeMap<String, Vec<String>>,
}

impl<'a> Scope<'a> {
    pub(crate) fn child(&self) -> Self {
        Self {
            table: self.table,
            locals: self.locals.clone(),
            generics: self.generics.clone(),
        }
    }

    pub(crate) fn from(&self, spec: &Spec) -> Type {
        match spec {
            Spec::Named {
                module,
                name,
                arguments,
            } => {
                if let Some(prefix) = module {
                    return Type::Nominal(format!("{prefix}.{name}"));
                }
                if name == "Fiber"
                    && let [arg] = arguments.as_slice()
                {
                    return Type::Fiber(Box::new(self.from(arg)));
                }
                if name == "List"
                    && let [arg] = arguments.as_slice()
                {
                    return Type::List(Box::new(self.from(arg)));
                }
                if self.generics.contains_key(name) {
                    return Type::Generic(name.clone());
                }
                if scalar(name) {
                    return Type::Scalar(name.clone());
                }
                Type::Nominal(self.table.resolve(name))
            }
            Spec::Borrow { mutable, target } => Type::Borrow {
                mutable: *mutable,
                inner: Box::new(self.from(target)),
            },
            Spec::Slice(inner) => Type::Slice(Box::new(self.from(inner))),
            Spec::Callable { .. } => Type::Unknown,
        }
    }

    pub(crate) fn items(&mut self, list: &[Item]) -> Result<Type, Finding> {
        let mut last = Type::Unknown;
        for item in list {
            last = self.statement(item)?;
        }
        Ok(last)
    }

    fn statement(&mut self, item: &Item) -> Result<Type, Finding> {
        match &item.statement {
            Statement::Bind(binding) => self.bind(binding, (item.line, item.column)),
            Statement::Conform(conf) => {
                for entry in &conf.methods {
                    self.infer(&entry.value)?;
                }
                Ok(Type::Unknown)
            }
            Statement::Assign { target, value, .. } => {
                let lhs = self.infer(target)?;
                let rhs = self.infer(value)?;
                if !compatible(&lhs, &rhs) {
                    return Err(Finding {
                        code: Code::Coercion,
                        line: item.line,
                        column: item.column,
                    });
                }
                Ok(Type::Unknown)
            }
            Statement::For {
                item: name,
                source,
                body,
                ..
            } => {
                let kind = self.infer(source)?;
                let mut inner = self.child();
                let elem = if matches!(kind, Type::Integer) {
                    Type::Integer
                } else {
                    Type::Unknown
                };
                inner.locals.insert(name.clone(), elem);
                inner.block(body)?;
                Ok(Type::Unknown)
            }
            Statement::Loop(kind) => self.repetition(kind),
            Statement::Evaluate(expr) => self.infer(expr),
            _ => Ok(Type::Unknown),
        }
    }

    fn bind(&mut self, binding: &Binding, pos: (usize, usize)) -> Result<Type, Finding> {
        let actual = self.infer(&binding.value)?;
        let chosen = if let Some(spec) = &binding.annotation {
            let expected = self.from(spec);
            if !compatible(&expected, &actual) {
                return Err(Finding {
                    code: Code::Coercion,
                    line: pos.0,
                    column: pos.1,
                });
            }
            expected
        } else {
            actual
        };
        self.locals.insert(binding.name.clone(), chosen);
        Ok(Type::Unknown)
    }

    pub(crate) fn block(&mut self, block: &Block) -> Result<Type, Finding> {
        self.child().items(&block.items)
    }

    pub(crate) fn repetition(&mut self, kind: &Loop) -> Result<Type, Finding> {
        match kind {
            Loop::Block(block) => self.block(block),
            Loop::Switch(dispatch) => self.dispatch(dispatch),
            Loop::Bucket(partition) => self.partition(partition),
        }
    }

    pub(crate) fn dispatch(&mut self, dispatch: &Dispatch) -> Result<Type, Finding> {
        let subject = self.infer(&dispatch.subject)?;
        if let Type::Nominal(name) = &subject
            && self.table.structs.contains_key(name)
        {
            return Err(Finding {
                code: Code::Scrutinee,
                line: dispatch.line,
                column: dispatch.column,
            });
        }
        let mut out = Type::Unknown;
        for arm in &dispatch.arms {
            out = self.branch(arm)?;
        }
        Ok(out)
    }

    pub(crate) fn partition(&mut self, partition: &Partition) -> Result<Type, Finding> {
        if let Some(subject) = &partition.subject {
            self.infer(subject)?;
        }
        let mut out = Type::Unknown;
        for arm in &partition.arms {
            let current = self.branch(arm)?;
            if forbidden(&current, self.table) {
                return Err(Finding {
                    code: Code::Outcome,
                    line: arm.line,
                    column: arm.column,
                });
            }
            out = current;
        }
        Ok(out)
    }

    fn branch(&mut self, arm: &Branch) -> Result<Type, Finding> {
        let mut inner = self.child();
        match &arm.outcome {
            Outcome::Value(expr) | Outcome::Break(Some(expr)) => inner.infer(expr),
            Outcome::Break(None) | Outcome::Continue => Ok(Type::Unknown),
            Outcome::Block(block) => inner.block(block),
        }
    }

    pub(crate) fn body(&mut self, body: &Body) -> Result<Type, Finding> {
        match body {
            Body::Block(block) => self.items(&block.items),
            Body::Switch(dispatch) => self.dispatch(dispatch),
            Body::Bucket(partition) => self.partition(partition),
            Body::Loop(kind) => self.repetition(kind),
        }
    }
}

pub(crate) fn scalar(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
            | "Text"
            | "Bytes"
    )
}

pub(crate) fn integer(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
    )
}

pub(crate) fn compatible(expected: &Type, actual: &Type) -> bool {
    match (expected, actual) {
        (Type::Unknown, _) | (_, Type::Unknown) => true,
        (a, b) if a == b => true,
        (
            Type::Borrow {
                mutable: em,
                inner: ei,
            },
            Type::Borrow {
                mutable: am,
                inner: ai,
            },
        ) => (!*em || *am) && compatible(ei, ai),
        (Type::Slice(ei), Type::Slice(ai))
        | (Type::List(ei), Type::List(ai))
        | (Type::Fiber(ei), Type::Fiber(ai)) => compatible(ei, ai),
        (Type::Scalar(name), Type::Integer) | (Type::Integer, Type::Scalar(name)) => integer(name),
        (Type::Scalar(name), Type::Float) | (Type::Float, Type::Scalar(name)) => {
            matches!(name.as_str(), "f32" | "f64")
        }
        _ => false,
    }
}

pub(crate) fn unify(left: Type, right: Type) -> Type {
    match (left, right) {
        (Type::Scalar(name), _) | (_, Type::Scalar(name)) => Type::Scalar(name),
        (Type::Nominal(name), _) | (_, Type::Nominal(name)) => Type::Nominal(name),
        (a, _) => a,
    }
}

fn forbidden(kind: &Type, table: &Table) -> bool {
    match kind {
        Type::Integer
        | Type::Float
        | Type::Scalar(_)
        | Type::Borrow { .. }
        | Type::Slice(_)
        | Type::List(_)
        | Type::Fiber(_) => true,
        Type::Nominal(name) => table.structs.contains_key(name),
        _ => false,
    }
}
