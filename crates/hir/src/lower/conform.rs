use crate::eval::derive::synthesize;
use crate::front::numeric::encode;
use crate::front::types::Map;
use crate::front::world::Claim;
use crate::layout::{Mangle, Type};
use crate::lower::engine::{Engine, Spec};
use crate::tree::{Import, Table};
use simplex::tree as syntax;
use std::collections::BTreeMap;

type Slot = (String, Vec<(String, Type)>, Type);

impl<'a> Engine<'a> {
    pub fn boundaries(&mut self) {
        for ((stem, local), ext) in &self.world.externals {
            let abi = crate::eval::asset::trim(&ext.abi);
            let sym = crate::eval::asset::trim(&ext.symbol);
            let lib = (abi == "c").then(|| String::from("libc.so.6"));
            if let Some(name) = &lib {
                self.libraries.insert(name.clone());
            }
            let inputs = ext
                .inputs
                .iter()
                .map(|p| {
                    self.typing
                        .optional(stem, p.annotation.as_ref(), &Map::new())
                })
                .collect();
            let output = self.typing.optional(stem, ext.output.as_ref(), &Map::new());
            self.imports.insert(
                sym.clone(),
                Import {
                    local: local.clone(),
                    symbol: sym,
                    library: lib,
                    inputs,
                    output,
                },
            );
        }
        for leaf in &self.world.shared {
            let lib = format!("lib{leaf}.so");
            self.libraries.insert(lib.clone());
            self.external(leaf, &lib);
        }
    }

    fn external(&mut self, leaf: &str, lib: &str) {
        for ((stem, name), s) in &self.world.structs {
            if stem == leaf && s.dynamic && s.generics.is_empty() {
                self.typing.nominal(stem, name, &[]);
            }
        }
        for ((stem, name), e) in &self.world.enums {
            if stem == leaf && e.dynamic && e.generics.is_empty() {
                self.typing.nominal(stem, name, &[]);
            }
        }
        for ((stem, name), b) in &self.world.bindings {
            if stem != leaf || !b.dynamic {
                continue;
            }
            let syntax::Expression::Function { inputs, output, .. } = &b.value else {
                continue;
            };
            let params = inputs
                .iter()
                .map(|p| {
                    self.typing
                        .optional(stem, p.annotation.as_ref(), &Map::new())
                })
                .collect();
            let out = match output {
                Some(spec) => self.typing.resolve(stem, spec, &Map::new()),
                None => {
                    let (empty, none) = (Map::new(), BTreeMap::new());
                    let probe = Spec {
                        expr: &b.value,
                        subst: &empty,
                        constants: &none,
                        exported: false,
                    };
                    let saved = self.routines.len();
                    let built = self.callable((stem, String::new()), &probe);
                    self.routines.truncate(saved);
                    built.output
                }
            };
            let symbol = Mangle::symbol(&self.ns, leaf, name);
            self.imports.insert(
                symbol.clone(),
                Import {
                    local: format!("{leaf}.{name}"),
                    symbol,
                    library: Some(lib.to_owned()),
                    inputs: params,
                    output: out,
                },
            );
        }
    }

    pub fn conformances(&mut self) {
        let claims = self.world.claims.clone();
        for claim in &claims {
            let pair = (claim.module.clone(), claim.subject.clone());
            let structs = &self.world.structs;
            let enums = &self.world.enums;
            let generic = structs.get(&pair).is_some_and(|s| !s.generics.is_empty())
                || enums.get(&pair).is_some_and(|e| !e.generics.is_empty());
            if !generic {
                self.table(claim, &[]);
            }
        }
    }

    pub fn table(&mut self, claim: &Claim, args: &[Type]) -> String {
        let subject = self.typing.nominal(&claim.module, &claim.subject, args);
        let label = encode(&subject);
        let contract = &claim.target.1;
        let sym = Mangle::table(&self.ns, &claim.module, &label, contract);
        if self.tables.contains_key(&sym) {
            return sym;
        }
        let subst = self.substitution(claim, &subject, args);
        let order = self.slots(claim, &subject);
        let mut methods = Vec::new();
        for (mname, inputs, output) in order {
            let msym = Mangle::method(&sym, &mname);
            methods.push(msym.clone());
            self.implement(claim, &subject, (&msym, &mname), (&subst, inputs, output));
        }
        if contract != "Copy" {
            self.tables.insert(
                sym.clone(),
                Table {
                    symbol: sym.clone(),
                    subject: label,
                    contract: contract.clone(),
                    methods,
                },
            );
        }
        sym
    }

    fn substitution(&self, claim: &Claim, subject: &Type, args: &[Type]) -> Map {
        let mut subst = Map::new();
        subst.insert(String::from("Self"), subject.clone());
        let structs = &self.world.structs;
        if let Some(def) = structs.get(&(claim.module.clone(), claim.subject.clone())) {
            for (g, t) in def.generics.iter().zip(args) {
                subst.insert(g.name.clone(), t.clone());
            }
        }
        subst
    }

    fn implement(
        &mut self,
        claim: &Claim,
        subject: &Type,
        names: (&str, &str),
        sig: (&Map, Vec<(String, Type)>, Type),
    ) {
        let (msym, mname) = names;
        if !self.seen.insert(msym.to_owned()) {
            return;
        }
        if claim.derived {
            let routine = synthesize(
                (msym.to_owned(), subject),
                (sig.1, sig.2),
                &self.typing.catalog,
            );
            self.routines.push(routine);
            return;
        }
        if let Some(expr) = claim.methods.get(mname).cloned() {
            let none = BTreeMap::new();
            let spec = Spec {
                expr: &expr,
                subst: sig.0,
                constants: &none,
                exported: self.shared,
            };
            let routine = self.callable((&claim.module, msym.to_owned()), &spec);
            self.routines.push(routine);
        }
    }

    fn slots(&mut self, claim: &Claim, subject: &Type) -> Vec<Slot> {
        let interfaces = &self.world.interfaces;
        let Some(contract) = interfaces.get(&claim.target).cloned() else {
            return claim
                .methods
                .keys()
                .map(|k| (k.clone(), Vec::new(), Type::Void))
                .collect();
        };
        let mut subst = Map::new();
        subst.insert(String::from("Self"), subject.clone());
        contract
            .methods
            .iter()
            .map(|r| {
                let ins = r
                    .inputs
                    .iter()
                    .map(|p| {
                        (
                            p.name.clone(),
                            self.typing
                                .optional(&claim.target.0, p.annotation.as_ref(), &subst),
                        )
                    })
                    .collect();
                let out = self
                    .typing
                    .optional(&claim.target.0, r.output.as_ref(), &subst);
                (r.name.clone(), ins, out)
            })
            .collect()
    }
}
