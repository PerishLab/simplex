use crate::code::{Code, Finding};
use crate::model::{Signature, Table};
use crate::tree::{
    Binding, Contract, Decorator, Expression, Generic, Parameter, Program, Spec, Statement,
};

impl Table {
    pub(crate) fn decorators(&mut self, program: &Program) -> Result<(), Finding> {
        for item in &program.items {
            self.declaration(&item.statement)?;
        }
        for item in &program.items {
            self.intrinsics(item, false)?;
        }
        Ok(())
    }

    fn declaration(&mut self, stmt: &Statement) -> Result<(), Finding> {
        match stmt {
            Statement::Struct(s) => {
                let (name, decs) = (s.name.clone(), s.decorators.clone());
                self.annotated(&name, &decs)?;
                for slot in &s.fields {
                    tags(&slot.decorators)?;
                }
                Ok(())
            }
            Statement::Enum(e) => {
                let (name, decs) = (e.name.clone(), e.decorators.clone());
                self.annotated(&name, &decs)?;
                for variant in &e.variants {
                    tags(&variant.decorators)?;
                }
                Ok(())
            }
            Statement::Bind(b) => self.decorated(b),
            _ => Ok(()),
        }
    }

    fn annotated(&mut self, owner: &str, list: &[Decorator]) -> Result<(), Finding> {
        for dec in list {
            if dec.module.is_some() {
                continue;
            }
            if dec.name != "derive" {
                return Err(fault(Code::Shape, dec));
            }
            if dec.arguments.is_empty() {
                return Err(fault(Code::Placement, dec));
            }
            for arg in &dec.arguments {
                self.derive(owner, arg, dec)?;
            }
        }
        Ok(())
    }

    fn derive(&mut self, owner: &str, arg: &Expression, dec: &Decorator) -> Result<(), Finding> {
        let Expression::Word { text, .. } = arg else {
            return Err(fault(Code::Placement, dec));
        };
        if !text
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            return Err(fault(Code::Placement, dec));
        }
        let target = self.resolve(text);
        if self.structs.contains_key(&target) || self.enums.contains_key(&target) {
            return Err(fault(Code::Placement, dec));
        }
        if target == "Copy" {
            self.copies.insert(owner.to_owned());
        }
        if let Some(contract) = self.interfaces.get(&target).cloned() {
            self.synthesize(owner, &contract);
        }
        self.satisfied.insert((owner.to_owned(), target));
        Ok(())
    }

    fn synthesize(&mut self, owner: &str, contract: &Contract) {
        for req in &contract.methods {
            let sig = Signature {
                asynchronous: req.asynchronous,
                generics: Vec::new(),
                inputs: req.inputs.iter().map(|p| p.annotation.clone()).collect(),
                output: req.output.clone(),
            };
            self.methods
                .insert((owner.to_owned(), req.name.clone()), sig);
        }
    }

    fn decorated(&mut self, b: &Binding) -> Result<(), Finding> {
        if matches!(
            &b.value,
            Expression::Call { callee, .. }
                if matches!(callee.as_ref(), Expression::Word { text, .. } if text == "embed")
        ) {
            self.assets.insert(b.name.clone());
        }
        if b.decorators.is_empty() {
            return Ok(());
        }
        let Expression::Function { inputs, output, .. } = &b.value else {
            return Err(fault(Code::Placement, &b.decorators[0]));
        };
        for dec in &b.decorators {
            self.entry(&b.name, dec, (inputs, output.as_ref()))?;
        }
        Ok(())
    }

    fn entry(
        &mut self,
        owner: &str,
        dec: &Decorator,
        sig: (&[Parameter], Option<&Spec>),
    ) -> Result<(), Finding> {
        if dec.module.is_some() {
            return Ok(());
        }
        match dec.name.as_str() {
            "derive" => Err(fault(Code::Placement, dec)),
            "caller" if !dec.arguments.is_empty() => Err(fault(Code::Placement, dec)),
            "caller" => {
                self.callers.insert(owner.to_owned());
                Ok(())
            }
            _ => self.aspect(dec, sig.0, sig.1),
        }
    }

    fn aspect(
        &self,
        dec: &Decorator,
        inputs: &[Parameter],
        output: Option<&Spec>,
    ) -> Result<(), Finding> {
        let Some(wrapper) = self.functions.get(&dec.name) else {
            return Err(fault(Code::Wrapper, dec));
        };
        if wrapper.asynchronous
            || self.externals.contains(&dec.name)
            || wrapper.inputs.len() != dec.arguments.len() + 2
        {
            return Err(fault(Code::Wrapper, dec));
        }
        let span = wrapper.inputs[dec.arguments.len()].as_ref();
        if span.is_none_or(|s| self.nominal(s).as_deref() != Some("Span")) {
            return Err(fault(Code::Wrapper, dec));
        }
        let last = wrapper.inputs.last().and_then(Option::as_ref);
        if !preserved(
            &wrapper.generics,
            last,
            wrapper.output.as_ref(),
            (inputs, output),
        ) {
            return Err(fault(Code::Wrapper, dec));
        }
        Ok(())
    }
}

fn tags(list: &[Decorator]) -> Result<(), Finding> {
    for dec in list {
        if dec.module.is_none() && matches!(dec.name.as_str(), "derive" | "caller") {
            return Err(fault(Code::Placement, dec));
        }
    }
    Ok(())
}

fn preserved(
    generics: &[Generic],
    last: Option<&Spec>,
    ret: Option<&Spec>,
    target: (&[Parameter], Option<&Spec>),
) -> bool {
    let (Some(param), Some(out)) = (last, ret) else {
        return false;
    };
    if param != out {
        return false;
    }
    if let Spec::Callable { inputs, output, .. } = param {
        return inputs.len() == target.0.len()
            && inputs
                .iter()
                .zip(target.0)
                .all(|(a, b)| b.annotation.as_ref() == Some(a))
            && output.as_deref() == target.1;
    }
    let Some(expected) = target.1 else {
        return false;
    };
    if param == expected {
        return true;
    }
    matches!(
        param,
        Spec::Named { module: None, name, arguments }
            if arguments.is_empty() && generics.iter().any(|g| g.name == *name)
    )
}

fn fault(code: Code, dec: &Decorator) -> Finding {
    Finding {
        code,
        line: dec.line,
        column: dec.column,
    }
}
