//! Observers (M2 phase 2.0b, ADR-0069): processors that run on entities that gained or lost a
//! component, after each simulation phase's commands are applied. Mass-style (public docs only).

use std::collections::BTreeMap;

use concordia_core::id::EntityId;

use crate::component::Component;
use crate::processor::{Key, Phase, ProcessorDecl, processor};

/// Which change an observer reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observe {
    /// The entity gained the component.
    Add,
    /// The entity lost it (removed or despawned).
    Remove,
}

/// Starts an observer of `T` named `name`; it runs after each simulation phase in which some
/// entity gained (or lost) `T`, with those entities in [`crate::View::observed`].
pub fn observer<T: Component>(name: &str, on: Observe) -> ProcessorDecl {
    let mut decl = processor(name, Phase::PostPhysics);
    decl.trigger = Some((Key::of::<T>(), on));
    decl
}

/// Adds and removals of observed component types, in the order they happened.
#[derive(Clone, Default)]
pub(crate) struct StructuralLog {
    observed: Vec<&'static str>,
    events: Vec<(&'static str, EntityId, bool)>,
}

impl StructuralLog {
    /// Starts logging changes of `types` (only these cost anything).
    pub(crate) fn observe(&mut self, types: impl IntoIterator<Item = &'static str>) {
        for t in types {
            if !self.observed.contains(&t) {
                self.observed.push(t);
            }
        }
    }

    /// Records that `entity` gained (`added`) or lost component `name`, if it is observed.
    pub(crate) fn record(&mut self, name: &'static str, entity: EntityId, added: bool) {
        if !self.observed.is_empty() && self.observed.contains(&name) {
            self.events.push((name, entity, added));
        }
    }

    /// True if nothing is observed, so callers can skip work.
    pub(crate) fn is_idle(&self) -> bool {
        self.observed.is_empty()
    }

    /// Takes the log as net changes: per (type, change), the entities in id order. Adding then
    /// removing (or the reverse) in one batch cancels out; replacing never logs.
    pub(crate) fn take_net(&mut self) -> BTreeMap<(TypeKey, bool), Vec<EntityId>> {
        let mut first_last: BTreeMap<(TypeKey, EntityId), (bool, bool)> = BTreeMap::new();
        for (t, id, added) in self.events.drain(..) {
            first_last
                .entry((TypeKey(t), id))
                .and_modify(|e| e.1 = added)
                .or_insert((added, added));
        }
        let mut net: BTreeMap<(TypeKey, bool), Vec<EntityId>> = BTreeMap::new();
        for ((t, id), (first, last)) in first_last {
            let (had, has) = (!first, last);
            if had != has {
                net.entry((t, has)).or_default().push(id);
            }
        }
        net
    }
}

/// A component's stable name as a map key (the order never reaches results: entities are per
/// type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TypeKey(pub(crate) &'static str);
