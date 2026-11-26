use crate::front::types::{Map, Typing, field, pad, zip};
use crate::layout::{Arm, Footprint, Tagged, Type};
use simplex::tree::Enumeration;

impl<'a> Typing<'a> {
    pub fn choice(&mut self, owner: &str, name: &str, args: &[Type]) -> Option<Type> {
        let pair = (owner.to_owned(), name.to_owned());
        let enums = &self.world.enums;
        let def = enums.get(&pair).cloned()?;
        let key = self.label(owner, name, args);
        if !self.catalog.choices.contains_key(&key) {
            self.owners.insert(
                key.clone(),
                (owner.to_owned(), name.to_owned(), args.to_vec()),
            );
            let tagged = self.enumeration(&key, owner, &def, args);
            self.catalog.choices.insert(key.clone(), tagged);
        }
        Some(Type::Choice(key))
    }

    fn enumeration(&mut self, key: &str, owner: &str, def: &Enumeration, args: &[Type]) -> Tagged {
        let local = zip(&def.generics, args);
        if def.variants.iter().all(|v| v.payloads.is_empty()) {
            let fp = match def.variants.is_empty() {
                true => Footprint::VOID,
                false => Footprint::scalar(1),
            };
            let arms = def
                .variants
                .iter()
                .enumerate()
                .map(|(i, v)| Arm {
                    name: v.name.clone(),
                    tag: i as u64,
                    payloads: Vec::new(),
                })
                .collect();
            return Tagged {
                name: key.to_owned(),
                discriminant: 1,
                footprint: fp,
                arms,
            };
        }
        self.payloads(key, owner, def, &local)
    }

    fn payloads(&mut self, key: &str, owner: &str, def: &Enumeration, local: &Map) -> Tagged {
        let (mut largest, mut align, mut arms) = (1u64, 1u64, Vec::new());
        for (idx, variant) in def.variants.iter().enumerate() {
            let (mut offset, mut payloads) = (1u64, Vec::new());
            for (pos, spec) in variant.payloads.iter().enumerate() {
                let kind = self.resolve(owner, spec, local);
                let fp = kind.footprint(&self.catalog);
                let step = fp.align.max(1);
                offset = pad(offset, step);
                payloads.push(field(&pos.to_string(), offset, kind));
                offset += fp.size;
                align = align.max(step);
            }
            largest = largest.max(offset);
            arms.push(Arm {
                name: variant.name.clone(),
                tag: idx as u64,
                payloads,
            });
        }
        Tagged {
            name: key.to_owned(),
            discriminant: 1,
            footprint: Footprint::aggregate(pad(largest, align), align),
            arms,
        }
    }
}
