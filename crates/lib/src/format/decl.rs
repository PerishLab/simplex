use super::Printer;
use crate::tree::{Alias, Contract, Enumeration, Generic, Method, Spec, Structure};

impl Printer {
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

    pub(crate) fn method(&mut self, item: &Method) {
        self.out.push_str(&item.receiver);
        self.out.push('.');
        self.out.push_str(&item.name);
        self.out.push_str(" := ");
        self.expression(&item.value);
    }

    pub(crate) fn structure(&mut self, item: &Structure) {
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
        self.out.push_str(&item.name);
        self.clauses(&item.generics);
        self.out.push_str(" := enum {\n");
        self.indent += 1;
        for (index, variant) in item.variants.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
            }
            self.margin();
            self.out.push_str(&variant.name);
            if !variant.payloads.is_empty() {
                self.out.push('(');
                self.specs(&variant.payloads);
                self.out.push(')');
            }
        }
        self.indent -= 1;
        self.out.push('\n');
        self.margin();
        self.out.push('}');
    }

    pub(crate) fn contract(&mut self, item: &Contract) {
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
        self.out.push_str(&item.name);
        self.clauses(&item.generics);
        self.out.push_str(" := ");
        self.spec(&item.target);
    }

    pub(crate) fn conform(&mut self, subject: &str, contract: &Spec) {
        self.out.push_str(subject);
        self.out.push_str(" : ");
        self.spec(contract);
    }
}
