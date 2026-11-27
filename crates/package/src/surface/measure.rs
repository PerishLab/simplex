use super::resolve::{Catalog, Subst, bind};
use super::types::{Footprint, Kind, pad};
use simplex::tree::{Enumeration, Spec, Structure};

impl<'a> Catalog<'a> {
    pub(crate) fn record(
        &mut self,
        owner: &str,
        def: &Structure,
        args: &[Kind],
    ) -> (Footprint, Vec<String>) {
        let subst = bind(&def.generics, args);
        let (mut offset, mut align, mut fields) = (0u64, 1u64, Vec::new());
        for slot in &def.fields {
            let kind = self.resolve(owner, &slot.spec, &subst, 0);
            let fp = self.footprint(&kind);
            let step = fp.align.max(1);
            offset = pad(offset, step);
            let tag = match slot.public {
                true => format!(
                    "pub:{}@{offset}={}({})",
                    slot.name,
                    kind.label(),
                    fp.render()
                ),
                false => format!("priv@{offset}={}({})", kind.label(), fp.render()),
            };
            fields.push(tag);
            offset += fp.size;
            align = align.max(step);
        }
        let size = if offset == 0 { 0 } else { pad(offset, align) };
        (Footprint::aggregate(size, align), fields)
    }

    pub(crate) fn choice(
        &mut self,
        owner: &str,
        def: &Enumeration,
        args: &[Kind],
    ) -> (Footprint, Vec<String>) {
        let subst = bind(&def.generics, args);
        if def.variants.iter().all(|arm| arm.payloads.is_empty()) {
            let fp = if def.variants.is_empty() {
                Footprint::VOID
            } else {
                Footprint::scalar(1)
            };
            let arms = def
                .variants
                .iter()
                .enumerate()
                .map(|(tag, arm)| format!("{tag}:{}", arm.name))
                .collect();
            return (fp, arms);
        }
        let (mut largest, mut align, mut arms) = (1u64, 1u64, Vec::new());
        for (tag, arm) in def.variants.iter().enumerate() {
            let (end, step, desc) = self.variant(owner, &arm.payloads, &subst);
            largest = largest.max(end);
            align = align.max(step);
            arms.push(format!("{tag}:{}({desc})", arm.name));
        }
        (Footprint::aggregate(pad(largest, align), align), arms)
    }

    fn variant(&mut self, owner: &str, payloads: &[Spec], subst: &Subst) -> (u64, u64, String) {
        let (mut offset, mut align, mut items) = (1u64, 1u64, Vec::new());
        for (pos, spec) in payloads.iter().enumerate() {
            let kind = self.resolve(owner, spec, subst, 0);
            let fp = self.footprint(&kind);
            let step = fp.align.max(1);
            offset = pad(offset, step);
            items.push(format!("{pos}@{offset}={}({})", kind.label(), fp.render()));
            offset += fp.size;
            align = align.max(step);
        }
        (offset, align, items.join(","))
    }

    pub(crate) fn footprint(&mut self, kind: &Kind) -> Footprint {
        match kind {
            Kind::Void => Footprint::VOID,
            Kind::Bool | Kind::Signed(1) | Kind::Unsigned(1) => Footprint::scalar(1),
            Kind::Signed(2) | Kind::Unsigned(2) => Footprint::scalar(2),
            Kind::Signed(4) | Kind::Unsigned(4) | Kind::Float(4) => Footprint::scalar(4),
            Kind::Signed(_) | Kind::Unsigned(_) | Kind::Float(_) => Footprint::scalar(8),
            Kind::Pointer(_) | Kind::Fiber(_) | Kind::Function(_, _) => Footprint::scalar(8),
            Kind::Slice(_) | Kind::Trait(_) | Kind::List(_) => Footprint::pair(),
            Kind::Origin => Footprint::aggregate(64, 8),
            Kind::Param(_) | Kind::External(_, _) => Footprint::scalar(8),
            Kind::Record(label, args) => self.aggregate("S", label, args, true),
            Kind::Choice(label, args) => self.aggregate("E", label, args, false),
        }
    }

    fn aggregate(&mut self, tag: &str, label: &str, args: &[Kind], record: bool) -> Footprint {
        let token = format!("{tag}:{label}:{}", args.len());
        if !self.active.insert(token.clone()) {
            return Footprint::VOID;
        }
        let (owner, name) = split(label);
        let key = (owner.clone(), name);
        let fp = match record {
            true => self
                .structs
                .get(&key)
                .copied()
                .map_or(Footprint::scalar(8), |def| self.record(&owner, def, args).0),
            false => self
                .enums
                .get(&key)
                .copied()
                .map_or(Footprint::scalar(8), |def| self.choice(&owner, def, args).0),
        };
        self.active.remove(&token);
        fp
    }
}

fn split(qualified: &str) -> (String, String) {
    match qualified.rsplit_once('.') {
        Some((owner, name)) => (owner.to_owned(), name.to_owned()),
        None => (String::new(), qualified.to_owned()),
    }
}
