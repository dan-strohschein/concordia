//! Entity handles and the table mapping stable ids to storage locations.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use concordia_core::id::EntityId;

use crate::error::EcsError;

/// Fast internal handle; stale handles are detected by generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity {
    index: u32,
    generation: NonZeroU32,
}

/// Where an entity's components live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Location {
    pub(crate) archetype: usize,
    pub(crate) row: usize,
}

#[derive(Clone)]
struct Slot {
    generation: NonZeroU32,
    id: Option<EntityId>,
    location: Location,
    /// The world's change tick when this entity last changed.
    changed: u64,
}

/// Slots plus the id → handle map (sorted, so iteration is deterministic).
#[derive(Clone, Default)]
pub(crate) struct EntityTable {
    slots: Vec<Slot>,
    free: Vec<u32>,
    ids: BTreeMap<EntityId, Entity>,
}

impl EntityTable {
    pub(crate) fn len(&self) -> usize {
        self.ids.len()
    }

    pub(crate) fn contains(&self, id: EntityId) -> bool {
        self.ids.contains_key(&id)
    }

    pub(crate) fn handle(&self, id: EntityId) -> Option<Entity> {
        self.ids.get(&id).copied()
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.ids.keys().copied()
    }

    pub(crate) fn id_of(&self, entity: Entity) -> Option<EntityId> {
        let slot = self.slots.get(entity.index as usize)?;
        (slot.generation == entity.generation)
            .then_some(slot.id)
            .flatten()
    }

    pub(crate) fn location(&self, id: EntityId) -> Result<Location, EcsError> {
        let entity = self.ids.get(&id).ok_or(EcsError::NoSuchEntity(id))?;
        Ok(self.slots[entity.index as usize].location)
    }

    /// Records that `id` changed at `tick`.
    pub(crate) fn touch(&mut self, id: EntityId, tick: u64) {
        if let Some(entity) = self.ids.get(&id) {
            self.slots[entity.index as usize].changed = tick;
        }
    }

    /// Entities changed after `tick`, in id order.
    pub(crate) fn changed_since(&self, tick: u64) -> Vec<EntityId> {
        self.ids
            .iter()
            .filter(|(_, e)| self.slots[e.index as usize].changed > tick)
            .map(|(id, _)| *id)
            .collect()
    }

    pub(crate) fn set_location(&mut self, id: EntityId, location: Location) {
        if let Some(entity) = self.ids.get(&id) {
            self.slots[entity.index as usize].location = location;
        }
    }

    pub(crate) fn alloc(&mut self, id: EntityId, location: Location) -> Result<Entity, EcsError> {
        debug_assert!(!self.contains(id), "caller checks for duplicates");
        let entity = match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                slot.id = Some(id);
                slot.location = location;
                slot.changed = 0;
                Entity {
                    index,
                    generation: slot.generation,
                }
            }
            None => self.push_slot(id, location)?,
        };
        self.ids.insert(id, entity);
        Ok(entity)
    }

    fn push_slot(&mut self, id: EntityId, location: Location) -> Result<Entity, EcsError> {
        let index = u32::try_from(self.slots.len()).map_err(|_| EcsError::TooManyEntities)?;
        let generation = NonZeroU32::MIN;
        self.slots.push(Slot {
            generation,
            id: Some(id),
            location,
            changed: 0,
        });
        Ok(Entity { index, generation })
    }

    /// Frees `id`'s slot; a slot whose generation would overflow is retired.
    pub(crate) fn free(&mut self, id: EntityId) {
        let Some(entity) = self.ids.remove(&id) else {
            return;
        };
        let slot = &mut self.slots[entity.index as usize];
        slot.id = None;
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(entity.index);
        }
    }
}
