//! Storage kinds (M2 phase 2.0b, ADR-0069): dense columns, shared values and tags, plus chunk
//! components. Every kind hashes exactly like a dense component, except chunk components, which
//! are derived data and never hashed.

use std::any::TypeId;

use concordia_core::id::EntityId;

use super::chunk::TypedChunks;
use crate::component::Component;
use crate::error::EcsError;
use crate::world::World;

/// Rows per logical chunk of an archetype table (chunk components hold one value per chunk).
pub const CHUNK_ROWS: usize = 1024;

/// How a component type is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    /// One value per entity, in a column.
    Dense,
    /// One value per archetype; entities with different values live in different archetypes.
    Shared,
    /// Zero-sized: in the archetype's key, with no column.
    Tag,
}

/// Storage counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StorageStats {
    /// Archetypes, including the empty one.
    pub archetypes: usize,
    /// Component columns across all archetypes.
    pub columns: usize,
    /// Distinct stored shared and tag values.
    pub shared_values: usize,
}

/// How one entity is stored (for inspection tools).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityStorage {
    /// Archetype index.
    pub archetype: usize,
    /// Logical chunk index within the archetype.
    pub chunk: usize,
    /// Components in stable-name order, with their storage kind.
    pub components: Vec<(&'static str, StorageKind)>,
    /// Pending signals raised on it, by type name and count.
    pub pending_signals: Vec<(&'static str, usize)>,
}

impl World {
    /// How `id` is stored, or `None` if it is not alive.
    pub fn entity_storage(&self, id: EntityId) -> Option<EntityStorage> {
        let loc = self.entities.location(id).ok()?;
        let arch = &self.archetypes.list[loc.archetype];
        let components = arch
            .by_name
            .iter()
            .map(|&slot| {
                let info = self.components.info(arch.component_at(slot));
                (info.name, info.kind)
            })
            .collect();
        Some(EntityStorage {
            archetype: loc.archetype,
            chunk: loc.row / CHUNK_ROWS,
            components,
            pending_signals: self.signals.pending_for(id),
        })
    }

    /// Stores `T` as a shared component; must come before `T`'s first use.
    pub fn register_shared<T: Component>(&mut self) -> Result<(), EcsError> {
        let component = T::type_info().name;
        if let Some(cid) = self.components.id_of(TypeId::of::<T>()) {
            return match self.components.info(cid).kind {
                StorageKind::Shared => Ok(()),
                _ => Err(EcsError::StorageKindInUse { component }),
            };
        }
        self.components.register_as::<T>(StorageKind::Shared)?;
        Ok(())
    }

    /// How `T` is stored, if the world has seen it.
    pub fn storage_kind<T: Component>(&self) -> Option<StorageKind> {
        let cid = self.components.id_of(TypeId::of::<T>())?;
        Some(self.components.info(cid).kind)
    }

    /// Storage counters.
    pub fn storage_stats(&self) -> StorageStats {
        StorageStats {
            archetypes: self.archetypes.list.len(),
            columns: self
                .archetypes
                .list
                .iter()
                .map(|a| a.components.len())
                .sum(),
            shared_values: self.shared.len(),
        }
    }

    /// The chunk component `C` of the chunk holding `id`.
    pub fn chunk<C: Component>(&self, id: EntityId) -> Option<&C> {
        let loc = self.entities.location(id).ok()?;
        let arch = &self.archetypes.list[loc.archetype];
        let (_, column) = arch.chunks.iter().find(|(t, _)| *t == TypeId::of::<C>())?;
        let typed = column.as_any().downcast_ref::<TypedChunks<C>>()?;
        typed.0.get(loc.row / CHUNK_ROWS)?.as_ref()
    }

    /// Sets the chunk component `C` of the chunk holding `id`; returns the previous value.
    pub fn set_chunk<C: Component>(
        &mut self,
        id: EntityId,
        value: C,
    ) -> Result<Option<C>, EcsError> {
        let loc = self.entities.location(id)?;
        let arch = &mut self.archetypes.list[loc.archetype];
        let index = match arch
            .chunks
            .iter()
            .position(|(t, _)| *t == TypeId::of::<C>())
        {
            Some(i) => i,
            None => {
                arch.chunks
                    .push((TypeId::of::<C>(), Box::new(TypedChunks::<C>(Vec::new()))));
                arch.chunks.len() - 1
            }
        };
        let typed = arch.chunks[index]
            .1
            .as_any_mut()
            .downcast_mut::<TypedChunks<C>>()
            .expect("chunk column matches its type id");
        let chunk = loc.row / CHUNK_ROWS;
        if typed.0.len() <= chunk {
            typed.0.resize_with(chunk + 1, || None);
        }
        Ok(typed.0[chunk].replace(value))
    }
}
