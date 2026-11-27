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
    /// Appends a payload of another Rust type with the same stable name (a typed and a dynamic
    /// raise of one signal), converted through its JSON form; false if it doesn't convert.
    fn push_dyn(&mut self, target: EntityId, value: &dyn DynReflect) -> bool;
}

/// A signal type's list, with how to convert a payload raised as another Rust type into `S`.
struct Typed<S> {
    items: Vec<(EntityId, S)>,
    from_dyn: fn(&dyn DynReflect) -> Option<S>,
}

/// A payload as JSON.
fn json_of(value: &dyn DynReflect) -> Option<serde_json::Value> {
    serde_json::to_value(value.as_serialize()).ok()
}

/// Any reflected payload as the Rust type `S`.
fn concrete<S: Component>(value: &dyn DynReflect) -> Option<S> {
    serde_json::from_value(json_of(value)?).ok()
}

/// Any reflected payload as a dynamic value of its own type.
fn dynamic(value: &dyn DynReflect) -> Option<DynamicValue> {
    DynamicValue::from_json(value.info(), json_of(value)?).ok()
}

impl<S: DynReflect + Clone> SignalList for Typed<S> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn SignalList> {
        Box::new(Typed {
            items: self.items.clone(),
            from_dyn: self.from_dyn,
        })
    }
    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    fn count_for(&self, id: EntityId) -> usize {
        self.items.iter().filter(|(t, _)| *t == id).count()
    }
    fn prepare(&mut self, entities: &EntityTable) {
        self.items.retain(|(id, _)| entities.contains(*id));
        self.items.sort_by_key(|(id, _)| *id);
    }
    fn hash(&self, h: &mut blake3::Hasher) -> Result<(), EcsError> {
        let mut order: Vec<usize> = (0..self.items.len()).collect();
        order.sort_by_key(|&i| self.items[i].0);
        h.update(&(order.len() as u64).to_le_bytes());
        for i in order {
            let (id, payload) = &self.items[i];
            h.update(id.as_uuid().as_bytes());
            canonical_hash(payload, h)?;
        }
        Ok(())
    }
    fn push_dyn(&mut self, target: EntityId, value: &dyn DynReflect) -> bool {
        match (self.from_dyn)(value) {
            Some(v) => {
                self.items.push((target, v));
                true
            }
            None => false,
        }
    }
    fn entries(&self) -> Vec<(EntityId, &dyn DynReflect)> {
        self.items
            .iter()
            .map(|(id, s)| (*id, s as &dyn DynReflect))
            .collect()
    }
}

type Lists = BTreeMap<&'static str, Box<dyn SignalList>>;

/// Converted pending and delivered payloads of one dynamic signal type (`None`: no such list).
pub(crate) type ConvertedSignals = [Option<Vec<(EntityId, DynamicValue)>>; 2];

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
    /// Raises a Rust-typed signal. A list raised dynamically first becomes typed, so typed
    /// readers see every raise.
    fn raise<S: Component>(&mut self, target: EntityId, signal: S) {
        let name = S::type_info().name;
        let typed = self.pending.get(name).map(|l| l.as_any().is::<Typed<S>>());
        if typed == Some(false) {
            let converted: Option<Vec<(EntityId, S)>> = self.pending[name]
                .entries()
                .into_iter()
                .map(|(id, v)| concrete::<S>(v).map(|s| (id, s)))
                .collect();
            if let Some(items) = converted {
                self.pending.insert(
                    name,
                    Box::new(Typed {
                        items,
                        from_dyn: concrete::<S>,
                    }),
                );
            }
        }
        self.raise_value(name, target, signal, concrete::<S>);
    }

    /// Raises a dynamic (script or API) signal.
    fn raise_dynamic(&mut self, target: EntityId, signal: DynamicValue) {
        self.raise_value(signal.info().name, target, signal, dynamic);
    }

    fn raise_value<S: DynReflect + Clone>(
        &mut self,
        name: &'static str,
        target: EntityId,
        signal: S,
        from_dyn: fn(&dyn DynReflect) -> Option<S>,
    ) {
        let list = self.pending.entry(name).or_insert_with(|| {
            Box::new(Typed::<S> {
                items: Vec::new(),
                from_dyn,
            })
        });
        match list.as_any_mut().downcast_mut::<Typed<S>>() {
            Some(typed) => typed.items.push((target, signal)),
            None => {
                // Types sharing a stable name are one type (the component index forbids others).
                let pushed = list.push_dyn(target, &signal);
                debug_assert!(pushed, "signal {name} raised as an incompatible Rust type");
            }
        }
    }

    /// The pending and delivered signals named `name` with payloads converted by `convert`
    /// (`None` where there is no such list of dynamic values); changes nothing.
    pub(crate) fn converted<E>(
        &self,
        name: &str,
        convert: &mut impl FnMut(&DynamicValue) -> Result<DynamicValue, E>,
    ) -> Result<ConvertedSignals, E> {
        let convert_list = |lists: &Lists,
                            convert: &mut dyn FnMut(&DynamicValue) -> Result<DynamicValue, E>|
         -> Result<Option<Vec<(EntityId, DynamicValue)>>, E> {
            let Some(typed) = lists
                .get(name)
                .and_then(|l| l.as_any().downcast_ref::<Typed<DynamicValue>>())
            else {
                return Ok(None);
            };
            let mut out = Vec::with_capacity(typed.items.len());
            for (id, payload) in &typed.items {
                out.push((*id, convert(payload)?));
            }
            Ok(Some(out))
        };
        Ok([
            convert_list(&self.pending, convert)?,
            convert_list(&self.delivered, convert)?,
        ])
    }

    /// Puts back lists made by [`SignalQueue::converted`].
    pub(crate) fn replace_converted(
        &mut self,
        name: &str,
        lists: [Option<Vec<(EntityId, DynamicValue)>>; 2],
    ) {
        let [pending, delivered] = lists;
        for (target, list) in [
            (&mut self.pending, pending),
            (&mut self.delivered, delivered),
        ] {
            if let Some(list) = list
                && let Some(typed) = target
                    .get_mut(name)
                    .and_then(|l| l.as_any_mut().downcast_mut::<Typed<DynamicValue>>())
            {
                typed.items = list;
            }
        }
    }

    /// Raised `S` signals not yet delivered, sorted by target (raise order kept per target).
    pub(crate) fn pending<S: Component>(&self) -> Vec<(EntityId, &S)> {
        let mut out: Vec<(EntityId, &S)> = self
            .pending
            .get(S::type_info().name)
            .and_then(|l| l.as_any().downcast_ref::<Typed<S>>())
            .map(|t| t.items.iter().map(|(id, s)| (*id, s)).collect())
            .unwrap_or_default();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    /// This tick's `S` signals.
    pub(crate) fn delivered<S: Component>(&self) -> &[(EntityId, S)] {
        self.delivered
            .get(S::type_info().name)
            .and_then(|l| l.as_any().downcast_ref::<Typed<S>>())
            .map_or(&[], |t| &t.items)
    }

    /// This tick's signals of the type named `name` (static or runtime-described).
    pub(crate) fn delivered_named(&self, name: &str) -> Vec<(EntityId, &dyn DynReflect)> {
        self.delivered
            .get(name)
            .map(|l| l.entries())
            .unwrap_or_default()
    }

    /// How many signals of the type named `name` are pending.
    pub(crate) fn pending_named_count(&self, name: &str) -> usize {
        self.pending.get(name).map_or(0, |l| l.entries().len())
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
        self.signals.raise_dynamic(target, signal);
    }

    /// This tick's delivered `S` signals, by entity then raise order.
    pub fn signals<S: Component>(&self) -> &[(EntityId, S)] {
        self.signals.delivered::<S>()
    }

    /// How many signals of the type named `name` were raised and not yet delivered (raised on
    /// the tick just run, when read after it).
    pub fn pending_signal_count(&self, name: &str) -> usize {
        self.signals.pending_named_count(name)
    }

    /// `S` signals raised and not yet delivered (they arrive next tick), sorted by entity then
    /// raise order; read-only, for inspection.
    pub fn pending_signals<S: Component>(&self) -> Vec<(EntityId, &S)> {
        self.signals.pending::<S>()
    }
}
