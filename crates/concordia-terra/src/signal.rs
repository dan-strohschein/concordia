//! Signals (M2 phase 2.0b, ADR-0069): typed values raised on entities and delivered on the next
//! tick, in entity order. Modeled on Mass signals (public docs only).

use std::any::Any;
use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_reflect::{DynReflect, DynamicValue, canonical_hash};

use crate::component::Component;
use crate::entity::EntityTable;
use crate::error::EcsError;
use crate::world::World;

/// One signal type's list of (target, payload), in raise order until delivered.
trait SignalList: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clone_box(&self) -> Box<dyn SignalList>;
    fn is_empty(&self) -> bool;
    /// Pending signals for `id`.
    fn count_for(&self, id: EntityId) -> usize;
    /// Sorts by target (stable, so raise order holds per target) and drops dead targets.
    fn prepare(&mut self, entities: &EntityTable);
    /// Hashes the list in delivery order.
    fn hash(&self, h: &mut blake3::Hasher) -> Result<(), EcsError>;
    /// Every (target, payload), in list order.
    fn entries(&self) -> Vec<(EntityId, &dyn DynReflect)>;
}

struct Typed<S>(Vec<(EntityId, S)>);

impl<S: DynReflect + Clone> SignalList for Typed<S> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn SignalList> {
        Box::new(Typed(self.0.clone()))
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn count_for(&self, id: EntityId) -> usize {
        self.0.iter().filter(|(t, _)| *t == id).count()
    }
    fn prepare(&mut self, entities: &EntityTable) {
        self.0.retain(|(id, _)| entities.contains(*id));
        self.0.sort_by_key(|(id, _)| *id);
    }
    fn hash(&self, h: &mut blake3::Hasher) -> Result<(), EcsError> {
        let mut order: Vec<usize> = (0..self.0.len()).collect();
        order.sort_by_key(|&i| self.0[i].0);
        h.update(&(order.len() as u64).to_le_bytes());
        for i in order {
            let (id, payload) = &self.0[i];
            h.update(id.as_uuid().as_bytes());
            canonical_hash(payload, h)?;
        }
        Ok(())
    }
    fn entries(&self) -> Vec<(EntityId, &dyn DynReflect)> {
        self.0
            .iter()
            .map(|(id, s)| (*id, s as &dyn DynReflect))
            .collect()
    }
}

type Lists = BTreeMap<&'static str, Box<dyn SignalList>>;

/// Pending (raised, not yet delivered) and delivered (this tick's) signals, by stable type name.
#[derive(Default)]
pub(crate) struct SignalQueue {
    pending: Lists,
    delivered: Lists,
}

impl Clone for SignalQueue {
    fn clone(&self) -> Self {
        let copy = |lists: &Lists| lists.iter().map(|(k, v)| (*k, v.clone_box())).collect();
        Self {
            pending: copy(&self.pending),
            delivered: copy(&self.delivered),
        }
    }
}

impl SignalQueue {
    fn raise<S: Component>(&mut self, target: EntityId, signal: S) {
        self.raise_value(S::type_info().name, target, signal);
    }

    fn raise_value<S: DynReflect + Clone>(
        &mut self,
        name: &'static str,
        target: EntityId,
        signal: S,
    ) {
        let list = self
            .pending
            .entry(name)
            .or_insert_with(|| Box::new(Typed::<S>(Vec::new())));
        // Two types sharing a stable name can't share a list; the component index forbids it too.
        if let Some(typed) = list.as_any_mut().downcast_mut::<Typed<S>>() {
            typed.0.push((target, signal));
        }
    }

    /// This tick's `S` signals.
    pub(crate) fn delivered<S: Component>(&self) -> &[(EntityId, S)] {
        self.delivered
            .get(S::type_info().name)
            .and_then(|l| l.as_any().downcast_ref::<Typed<S>>())
            .map_or(&[], |t| &t.0)
    }

    /// This tick's signals of the type named `name` (static or runtime-described).
    pub(crate) fn delivered_named(&self, name: &str) -> Vec<(EntityId, &dyn DynReflect)> {
        self.delivered
            .get(name)
            .map(|l| l.entries())
            .unwrap_or_default()
    }

    /// Pending signal types and counts for `id`, by type name.
    pub(crate) fn pending_for(&self, id: EntityId) -> Vec<(&'static str, usize)> {
        self.pending
            .iter()
            .map(|(name, list)| (*name, list.count_for(id)))
            .filter(|(_, n)| *n > 0)
            .collect()
    }

    /// Starts a tick: last tick's raises become this tick's deliveries, in entity order.
    pub(crate) fn deliver(&mut self, entities: &EntityTable) {
        self.delivered = std::mem::take(&mut self.pending);
        self.delivered.retain(|_, list| {
            list.prepare(entities);
            !list.is_empty()
        });
    }

    /// Hashes pending signals; adds nothing when there are none, so quiet worlds hash as before.
    pub(crate) fn hash(&self, h: &mut blake3::Hasher) -> Result<(), EcsError> {
        let lists: Vec<_> = self.pending.iter().filter(|(_, l)| !l.is_empty()).collect();
        if lists.is_empty() {
            return Ok(());
        }
        h.update(b"signals");
        h.update(&(lists.len() as u64).to_le_bytes());
        for (name, list) in lists {
            h.update(&(name.len() as u64).to_le_bytes());
            h.update(name.as_bytes());
            list.hash(h)?;
        }
        Ok(())
    }
}

impl World {
    /// Raises `signal` on `target`; it is delivered at the start of the next tick, if `target`
    /// is still alive.
    pub fn signal<S: Component>(&mut self, target: EntityId, signal: S) {
        self.signals.raise(target, signal);
    }

    /// Raises a signal of a runtime-described (script) type; delivered like [`World::signal`].
    pub fn signal_dynamic(&mut self, target: EntityId, signal: DynamicValue) {
        self.signals.raise_value(signal.info().name, target, signal);
    }

    /// This tick's delivered `S` signals, by entity then raise order.
    pub fn signals<S: Component>(&self) -> &[(EntityId, S)] {
        self.signals.delivered::<S>()
    }
}
