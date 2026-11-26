use super::Printer;
use crate::tree::{
    Alias, Conformance, Contract, Enumeration, External, Generic, Import, Structure,
};

impl Printer {
    pub(crate) fn imported(&mut self, item: &Import) {
        self.out.push_str(&item.name);
        self.out.push_str(" := import ");
        for (index, part) in item.path.iter().enumerate() {
            if index > 0 {
                self.out.push('.');
            }
            self.out.push_str(part);
        }
    }

    pub(crate) fn external(&mut self, item: &External) {
        if item.public {
            self.out.push_str("pub ");
        }
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
        if item.public {
            self.out.push_str("pub ");
        }
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
        if item.public {
            self.out.push_str("pub ");
        }
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
        if item.public {
            self.out.push_str("pub ");
        }
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
        if item.public {
            self.out.push_str("pub ");
        }
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
