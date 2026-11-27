use super::loader::{Graph, Unit};
use super::syntax::{generics, prefix, trim};
use simplex::tree::{
    Alias, Binding, Contract, Decorator, Enumeration, Expression, External, Spec, Statement,
    Structure,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn collect(graph: &Graph) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for unit in &graph.units {
        unit.exports(&mut out);
    }
    out
}

impl Unit {
    fn exports(&self, out: &mut BTreeMap<String, String>) {
        let copies = self.copied();
        let head = prefix(&self.id);
        for item in &self.program.items {
            self.item(&head, &copies, &item.statement, out);
        }
    }

    fn item(
        &self,
        head: &str,
        copies: &BTreeSet<String>,
        stmt: &Statement,
        out: &mut BTreeMap<String, String>,
    ) {
        match stmt {
            Statement::Struct(def) if def.public => {
                let copy = copies.contains(&def.name) || derived(&def.decorators, "Copy");
                out.insert(
                    format!("{head}struct:{}", def.name),
                    self.structure(def, copy),
                );
            }
            Statement::Enum(def) if def.public => {
                let copy = copies.contains(&def.name) || derived(&def.decorators, "Copy");
                out.insert(
                    format!("{head}enum:{}", def.name),
                    self.enumeration(def, copy),
                );
            }
            Statement::Interface(def) if def.public => {
                out.insert(format!("{head}interface:{}", def.name), self.interface(def));
            }
            Statement::Alias(def) if def.public => {
                out.insert(format!("{head}alias:{}", def.name), self.alias(def));
            }
            Statement::Extern(def) if def.public => {
                out.insert(format!("{head}extern:{}", def.name), self.external(def));
            }
            Statement::Bind(def) if def.public => {
                out.insert(format!("{head}bind:{}", def.name), self.binding(def));
            }
            _ => {}
        }
    }

    fn copied(&self) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        for item in &self.program.items {
            if let Statement::Conform(conf) = &item.statement
                && matches!(
                    &conf.contract,
                    Spec::Named {
                        module: None,
                        name,
                        ..
                    } if name == "Copy"
                )
            {
                set.insert(conf.subject.clone());
            }
        }
        set
    }

    fn structure(&self, def: &Structure, copy: bool) -> String {
        let constructible = def.fields.iter().all(|slot| slot.public);
        let fields: Vec<String> = def
            .fields
            .iter()
            .filter(|slot| slot.public)
            .map(|slot| format!("{}:{}", slot.name, self.spec(&slot.spec)))
            .collect();
        format!(
            "struct;dylib={};copy={copy};constructible={constructible};generics=[{}];fields=[{}]",
            def.dynamic,
            generics(&def.generics),
            fields.join(",")
        )
    }

    fn enumeration(&self, def: &Enumeration, copy: bool) -> String {
        let variants: Vec<String> = def
            .variants
            .iter()
            .map(|arm| {
                if arm.payloads.is_empty() {
                    return arm.name.clone();
                }
                let inner: Vec<String> = arm.payloads.iter().map(|item| self.spec(item)).collect();
                format!("{}({})", arm.name, inner.join(","))
            })
            .collect();
        format!(
            "enum;dylib={};copy={copy};generics=[{}];variants=[{}]",
            def.dynamic,
            generics(&def.generics),
            variants.join(",")
        )
    }

    fn interface(&self, def: &Contract) -> String {
        let methods: Vec<String> = def
            .methods
            .iter()
            .map(|req| {
                let ret = req
                    .output
                    .as_ref()
                    .map_or_else(|| String::from("void"), |item| self.spec(item));
                format!(
                    "{}:async={}({})->{}",
                    req.name,
                    req.asynchronous,
                    self.parameters(&req.inputs),
                    ret
                )
            })
            .collect();
        format!(
            "interface;dylib={};generics=[{}];methods=[{}]",
            def.dynamic,
            generics(&def.generics),
            methods.join(";")
        )
    }

    fn alias(&self, def: &Alias) -> String {
        format!(
            "alias;dylib={};generics=[{}];target={}",
            def.dynamic,
            generics(&def.generics),
            self.spec(&def.target)
        )
    }

    fn external(&self, def: &External) -> String {
        let ret = def
            .output
            .as_ref()
            .map_or_else(|| String::from("void"), |item| self.spec(item));
        format!(
            "extern;dylib={};abi={};symbol={};arity={};inputs=[{}];output={ret}",
            def.dynamic,
            trim(&def.abi),
            trim(&def.symbol),
            def.inputs.len(),
            self.parameters(&def.inputs)
        )
    }

    fn binding(&self, def: &Binding) -> String {
        let caller = def
            .decorators
            .iter()
            .any(|dec| dec.module.is_none() && dec.name == "caller");
        if let Expression::Function {
            asynchronous,
            generics: gens,
            inputs,
            output,
            body,
        } = &def.value
        {
            let ret = output
                .as_ref()
                .map_or_else(|| self.inferred(body), |item| self.spec(item));
            return format!(
                "fn;dylib={};mut={};async={asynchronous};caller={caller};generics=[{}];arity={};inputs=[{}];output={ret}",
                def.dynamic,
                def.mutable,
                generics(gens),
                inputs.len(),
                self.parameters(inputs)
            );
        }
        let kind = def
            .annotation
            .as_ref()
            .map_or_else(|| self.literal(&def.value), |item| self.spec(item));
        format!(
            "const;dylib={};mut={};type={kind}",
            def.dynamic, def.mutable
        )
    }
}

fn derived(list: &[Decorator], target: &str) -> bool {
    list.iter()
        .filter(|dec| dec.module.is_none() && dec.name == "derive")
        .flat_map(|dec| &dec.arguments)
        .any(|arg| matches!(arg, Expression::Word { text, .. } if text == target))
}
