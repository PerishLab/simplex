use super::loader::{Graph, Unit};
use super::resolve::{Catalog, Subst, placeholders};
use super::syntax::{generics, prefix, trim};
use super::types::{Kind, primitive};
use simplex::tree::{
    Binding, Contract, Enumeration, Expression, External, Parameter, Statement, Structure,
};
use std::collections::BTreeMap;

pub(crate) fn collect(graph: &Graph) -> BTreeMap<String, String> {
    let mut catalog = Catalog::new(graph);
    let mut out = BTreeMap::new();
    for unit in &graph.units {
        catalog.emit(unit, &mut out);
    }
    out
}

impl<'a> Catalog<'a> {
    fn emit(&mut self, unit: &Unit, out: &mut BTreeMap<String, String>) {
        let head = prefix(&unit.id);
        for item in &unit.program.items {
            match &item.statement {
                Statement::Struct(def) if def.public => {
                    let desc = self.structure(&unit.id, def);
                    out.insert(format!("{head}struct:{}", def.name), desc);
                }
                Statement::Enum(def) if def.public => {
                    let desc = self.enumeration(&unit.id, def);
                    out.insert(format!("{head}enum:{}", def.name), desc);
                }
                Statement::Interface(def) if def.public => {
                    let desc = self.interface(&unit.id, def);
                    out.insert(format!("{head}interface:{}", def.name), desc);
                }
                Statement::Alias(def) if def.public && def.dynamic => {
                    let kind = self.resolve(&unit.id, &def.target, &Subst::new(), 0);
                    let fp = self.footprint(&kind);
                    out.insert(
                        format!("{head}alias:{}", def.name),
                        format!("alias;dylib=true;target={}[{}]", kind.label(), fp.render()),
                    );
                }
                Statement::Extern(def) if def.public && def.dynamic => {
                    let desc = self.external(&unit.id, def);
                    out.insert(format!("{head}dylib:{}", def.name), desc);
                }
                Statement::Bind(def) if def.public && def.dynamic => {
                    let desc = self.binding(unit, def);
                    out.insert(format!("{head}dylib:{}", def.name), desc);
                }
                _ => {}
            }
        }
    }

    fn structure(&mut self, owner: &str, def: &Structure) -> String {
        if !def.generics.is_empty() {
            let subst = placeholders(&def.generics);
            let fields: Vec<String> = def
                .fields
                .iter()
                .map(|slot| {
                    let kind = self.resolve(owner, &slot.spec, &subst, 0);
                    match slot.public {
                        true => format!("pub:{}:{}", slot.name, kind.label()),
                        false => format!("priv:{}", kind.label()),
                    }
                })
                .collect();
            return format!(
                "struct;dylib={};generics=[{}];fields=[{}]",
                def.dynamic,
                generics(&def.generics),
                fields.join(",")
            );
        }
        let (fp, fields) = self.record(owner, def, &[]);
        format!(
            "struct;dylib={};size={};align={};passing={};fields=[{}]",
            def.dynamic,
            fp.size,
            fp.align,
            fp.passing.name(),
            fields.join(",")
        )
    }

    fn enumeration(&mut self, owner: &str, def: &Enumeration) -> String {
        if !def.generics.is_empty() {
            let subst = placeholders(&def.generics);
            let arms: Vec<String> = def
                .variants
                .iter()
                .enumerate()
                .map(|(tag, arm)| {
                    let items: Vec<String> = arm
                        .payloads
                        .iter()
                        .map(|spec| self.resolve(owner, spec, &subst, 0).label())
                        .collect();
                    format!("{tag}:{}({})", arm.name, items.join(","))
                })
                .collect();
            return format!(
                "enum;dylib={};discriminant=1;generics=[{}];variants=[{}]",
                def.dynamic,
                generics(&def.generics),
                arms.join(",")
            );
        }
        let (fp, arms) = self.choice(owner, def, &[]);
        format!(
            "enum;dylib={};discriminant=1;size={};align={};passing={};variants=[{}]",
            def.dynamic,
            fp.size,
            fp.align,
            fp.passing.name(),
            arms.join(",")
        )
    }

    fn interface(&mut self, owner: &str, def: &Contract) -> String {
        let mut subst = placeholders(&def.generics);
        subst.insert(String::from("Self"), Kind::Param(String::from("Self")));
        let slots: Vec<String> = def
            .methods
            .iter()
            .enumerate()
            .map(|(slot, req)| {
                let inputs = self.parameters(owner, &req.inputs, &subst);
                let raw = req
                    .output
                    .as_ref()
                    .map_or(Kind::Void, |spec| self.resolve(owner, spec, &subst, 0));
                let ret = if req.asynchronous {
                    Kind::Fiber(Box::new(raw))
                } else {
                    raw
                };
                let fp = self.footprint(&ret);
                format!(
                    "{slot}:{}:async={}({})->{}({})",
                    req.name,
                    req.asynchronous,
                    inputs,
                    ret.label(),
                    fp.render()
                )
            })
            .collect();
        format!(
            "interface;dylib={};generics=[{}];slots=[{}]",
            def.dynamic,
            generics(&def.generics),
            slots.join(";")
        )
    }

    fn external(&mut self, owner: &str, def: &External) -> String {
        let subst = Subst::new();
        let inputs = self.parameters(owner, &def.inputs, &subst);
        let ret = def
            .output
            .as_ref()
            .map_or(Kind::Void, |spec| self.resolve(owner, spec, &subst, 0));
        let fp = self.footprint(&ret);
        format!(
            "extern;abi={};symbol={};inputs=[{inputs}];output={}({})",
            trim(&def.abi),
            trim(&def.symbol),
            ret.label(),
            fp.render()
        )
    }

    fn binding(&mut self, unit: &Unit, def: &Binding) -> String {
        let subst = Subst::new();
        if let Expression::Function {
            asynchronous,
            inputs,
            output,
            body,
            ..
        } = &def.value
        {
            let mut params = self.parameters(&unit.id, inputs, &subst);
            let caller = def
                .decorators
                .iter()
                .any(|dec| dec.module.is_none() && dec.name == "caller");
            if caller {
                if !params.is_empty() {
                    params.push(',');
                }
                params.push_str("Origin(64/8/indirect)");
            }
            let raw = match output {
                Some(spec) => self.resolve(&unit.id, spec, &subst, 0),
                None => primitive(&unit.inferred(body)).unwrap_or(Kind::Void),
            };
            let ret = if *asynchronous {
                Kind::Fiber(Box::new(raw))
            } else {
                raw
            };
            let fp = self.footprint(&ret);
            return format!(
                "fn;async={asynchronous};inputs=[{params}];output={}({})",
                ret.label(),
                fp.render()
            );
        }
        let kind = match &def.annotation {
            Some(spec) => self.resolve(&unit.id, spec, &subst, 0),
            None => primitive(&unit.literal(&def.value)).unwrap_or(Kind::Signed(8)),
        };
        let fp = self.footprint(&kind);
        format!(
            "global;mut={};type={}({})",
            def.mutable,
            kind.label(),
            fp.render()
        )
    }

    fn parameters(&mut self, owner: &str, list: &[Parameter], subst: &Subst) -> String {
        let parts: Vec<String> = list
            .iter()
            .filter(|param| !param.constant)
            .map(|param| {
                let kind = param
                    .annotation
                    .as_ref()
                    .map_or(Kind::Signed(8), |spec| self.resolve(owner, spec, subst, 0));
                let fp = self.footprint(&kind);
                format!("{}({})", kind.label(), fp.render())
            })
            .collect();
        parts.join(",")
    }
}
