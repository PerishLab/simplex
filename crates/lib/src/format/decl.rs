use super::Printer;
use crate::tree::{
    Alias, Conformance, Contract, Decorator, Enumeration, External, Generic, Import, Parameter,
    Structure,
};

impl Printer {
    pub(crate) fn decorators(&mut self, list: &[Decorator]) {
        for dec in list {
            self.decorator(dec);
        }
    }

    fn decorator(&mut self, dec: &Decorator) {
        self.out.push('@');
        if let Some(module) = &dec.module {
            self.out.push_str(module);
            self.out.push('.');
        }
        self.out.push_str(&dec.name);
        if !dec.arguments.is_empty() {
            self.out.push('(');
            for (index, arg) in dec.arguments.iter().enumerate() {
                if index > 0 {
                    self.out.push_str(", ");
                }
                self.expression(arg);
            }
            self.out.push(')');
        }
        self.out.push('\n');
        self.margin();
    }

    pub(crate) fn visibility(&mut self, public: bool, dynamic: bool) {
        if dynamic {
            self.out.push_str("pub dylib ");
        } else if public {
            self.out.push_str("pub ");
        }
    }

    pub(crate) fn parameters(&mut self, inputs: &[Parameter]) {
        self.out.push('(');
        for (index, param) in inputs.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            if param.constant {
                self.out.push_str("comptime ");
            }
            self.out.push_str(&param.name);
            if let Some(spec) = &param.annotation {
                self.out.push_str(": ");
                self.spec(spec);
            }
        }
        self.out.push(')');
    }

    pub(crate) fn imported(&mut self, item: &Import) {
        self.out.push_str(&item.name);
        self.out.push_str(if item.dynamic {
            " := dylib "
        } else {
            " := import "
        });
        for (index, part) in item.path.iter().enumerate() {
            if index > 0 {
                self.out.push('.');
            }
            self.out.push_str(part);
        }
    }

    pub(crate) fn external(&mut self, item: &External) {
        self.visibility(item.public, item.dynamic);
        self.out.push_str(&item.name);
        self.out.push_str(" := extern ");
        self.out.push_str(&item.abi);
        self.out.push(' ');
        self.out.push_str(&item.symbol);
        self.out.push_str(" fn");
        self.parameters(&item.inputs);
        if let Some(output) = &item.output {
            self.out.push_str(" -> ");
            self.spec(output);
        }
    }

    pub(crate) fn clauses(&mut self, list: &[Generic]) {
        if list.is_empty() {
            return;
        }
        self.out.push('<');
        for (index, item) in list.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(&item.name);
            if !item.bounds.is_empty() {
                self.out.push_str(": ");
                self.bounds(&item.bounds);
            }
        }
        self.out.push('>');
    }

    fn bounds(&mut self, bounds: &[String]) {
        for (index, bound) in bounds.iter().enumerate() {
            if index > 0 {
                self.out.push_str(" + ");
            }
            self.out.push_str(bound);
        }
    }

    pub(crate) fn structure(&mut self, item: &Structure) {
        self.decorators(&item.decorators);
        self.visibility(item.public, item.dynamic);
        self.out.push_str(&item.name);
        self.clauses(&item.generics);
        if item.fields.is_empty() {
            self.out.push_str(" := struct {}");
            return;
        }
        self.out.push_str(" := struct {\n");
        self.indent += 1;
        for (index, field) in item.fields.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
            }
            self.margin();
            self.decorators(&field.decorators);
            if field.public {
                self.out.push_str("pub ");
            }
            self.out.push_str(&field.name);
            self.out.push_str(": ");
            self.spec(&field.spec);
        }
        self.indent -= 1;
        self.out.push('\n');
        self.margin();
        self.out.push('}');
    }

    pub(crate) fn enumeration(&mut self, item: &Enumeration) {
        self.decorators(&item.decorators);
        self.visibility(item.public, item.dynamic);
        self.out.push_str(&item.name);
        self.clauses(&item.generics);
        self.out.push_str(" := enum {\n");
        self.indent += 1;
        for (index, variant) in item.variants.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
            }
            self.margin();
            self.decorators(&variant.decorators);
            self.out.push_str(&variant.name);
            if !variant.payloads.is_empty() {
                self.out.push('(');
                self.specs(&item.variants[index].payloads);
                self.out.push(')');
            }
        }
        self.indent -= 1;
        self.out.push('\n');
        self.margin();
        self.out.push('}');
    }

    pub(crate) fn contract(&mut self, item: &Contract) {
        self.visibility(item.public, item.dynamic);
        self.out.push_str(&item.name);
        self.clauses(&item.generics);
        self.out.push_str(" := interface {\n");
        self.indent += 1;
        for (index, req) in item.methods.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
            }
            self.margin();
            self.out.push_str(&req.name);
            self.out.push_str(": ");
            if req.asynchronous {
                self.out.push_str("async ");
            }
            self.parameters(&req.inputs);
            if let Some(output) = &req.output {
                self.out.push_str(" -> ");
                self.spec(output);
            }
        }
        self.indent -= 1;
        self.out.push('\n');
        self.margin();
        self.out.push('}');
    }

    pub(crate) fn alias(&mut self, item: &Alias) {
        self.visibility(item.public, item.dynamic);
        self.out.push_str(&item.name);
        self.clauses(&item.generics);
        self.out.push_str(" := ");
        self.spec(&item.target);
    }

    pub(crate) fn conform(&mut self, item: &Conformance) {
        self.out.push_str(&item.subject);
        self.out.push_str(" : ");
        self.spec(&item.contract);
        if item.methods.is_empty() {
            self.out.push_str(" {}");
            return;
        }
        self.out.push_str(" {\n");
        self.indent += 1;
        for (index, entry) in item.methods.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
            }
            self.margin();
            self.out.push_str(&entry.name);
            self.out.push_str(": ");
            self.expression(&entry.value);
        }
        self.indent -= 1;
        self.out.push('\n');
        self.margin();
        self.out.push('}');
    }
}
