//! Structural checks on a module description (names, references, duplicates).

use std::collections::BTreeSet;

use crate::{ModuleDesc, ProcessorDesc, ProtocolError, TraitDesc};

fn invalid(message: String) -> ProtocolError {
    ProtocolError::InvalidDesc(message)
}

impl ModuleDesc {
    /// Checks names are non-empty and unique, processors don't name a component twice, and
    /// traits name only components. Type shapes are checked when they are registered.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.name.trim().is_empty() {
            return Err(invalid("the module name is empty".into()));
        }
        let types = self
            .types
            .iter()
            .chain(&self.components)
            .chain(&self.signals);
        unique("type", types.map(|t| t.name.as_str()))?;
        unique("processor", self.processors.iter().map(|p| p.name.as_str()))?;
        unique("trait", self.traits.iter().map(|t| t.name.as_str()))?;
        self.processors.iter().try_for_each(check_processor)?;
        self.traits.iter().try_for_each(check_trait)
    }
}

fn unique<'a>(what: &str, names: impl Iterator<Item = &'a str>) -> Result<(), ProtocolError> {
    let mut seen = BTreeSet::new();
    for name in names {
        if name.trim().is_empty() {
            return Err(invalid(format!("a {what} name is empty")));
        }
        if !seen.insert(name) {
            return Err(invalid(format!("two {what}s are named {name}")));
        }
    }
    Ok(())
}

fn check_processor(p: &ProcessorDesc) -> Result<(), ProtocolError> {
    let lists = [&p.reads, &p.writes, &p.with, &p.without];
    let mut seen = BTreeSet::new();
    for name in lists.into_iter().flatten() {
        if !seen.insert(name.as_str()) {
            return Err(invalid(format!(
                "processor {} names component {name} twice",
                p.name
            )));
        }
    }
    unique("signal", p.receives.iter().map(String::as_str))
        .map_err(|e| invalid(format!("processor {}: {e}", p.name)))
}

fn check_trait(t: &TraitDesc) -> Result<(), ProtocolError> {
    if t.components.is_empty() {
        return Err(invalid(format!("trait {} adds no components", t.name)));
    }
    unique("component", t.components.iter().map(String::as_str))
        .map_err(|e| invalid(format!("trait {}: {e}", t.name)))
}

#[cfg(test)]
mod tests {
    use crate::{ModuleDesc, ProcessorDesc, TraitDesc};

    fn desc() -> ModuleDesc {
        ModuleDesc {
            name: "m".into(),
            processors: vec![ProcessorDesc {
                name: "m.p".into(),
                reads: vec!["A".into()],
                writes: vec!["B".into()],
                ..ProcessorDesc::default()
            }],
            traits: vec![TraitDesc {
                name: "m.T".into(),
                components: vec!["B".into()],
                ..TraitDesc::default()
            }],
            ..ModuleDesc::default()
        }
    }

    #[test]
    fn a_well_formed_description_validates() {
        desc().validate().unwrap();
    }

    /// A word the error must contain, and how to break the description.
    type Case = (&'static str, fn(&mut ModuleDesc));

    #[test]
    fn malformed_descriptions_are_refused_with_the_reason() {
        let cases: [Case; 5] = [
            ("module name", |d| d.name.clear()),
            ("m.p", |d| d.processors.push(d.processors[0].clone())),
            ("twice", |d| d.processors[0].writes.push("A".into())),
            ("processor name", |d| d.processors[0].name.clear()),
            ("no components", |d| d.traits[0].components.clear()),
        ];
        for (needle, break_it) in cases {
            let mut d = desc();
            break_it(&mut d);
            let err = d.validate().expect_err(needle).to_string();
            assert!(err.contains(needle), "{needle}: {err}");
        }
    }
}
