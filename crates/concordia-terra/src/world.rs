//! The world: entities with stable ids, their components and resources.

use std::any::TypeId;
use std::sync::atomic::{AtomicU64, Ordering};

use concordia_core::id::{EntityId, IdSource};
use concordia_reflect::DynReflect;

use crate::component::{Component, ComponentId, ComponentIndex};
use crate::entity::{Entity, EntityTable, Location};
use crate::error::{EcsError, QueryError};
use crate::query::{QueryData, QueryFilter, QueryState};
use crate::resource::{Resource, Resources};
use crate::storage::archetype::Archetypes;
use crate::storage::column::{downcast, typed, typed_mut};

static NEXT_WORLD: AtomicU64 = AtomicU64::new(1);

/// Entities, components and resources. Every entity has a stable [`EntityId`].
pub struct World {
    pub(crate) uid: u64,
    pub(crate) entities: EntityTable,
    pub(crate) archetypes: Archetypes,
    pub(crate) components: ComponentIndex,
    pub(crate) resources: Resources,
    pub(crate) changes: ChangeLog,
}

/// The world's change clock and recent despawns.
#[derive(Clone, Default)]
pub(crate) struct ChangeLog {
    tick: u64,
    despawned: std::collections::VecDeque<(u64, EntityId)>,
    /// Changes at or before this tick may be missing from `despawned`.
    floor: u64,
}

impl ChangeLog {
    pub(crate) fn bump(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }
}

/// What changed in a world since a tick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// Entities spawned or changed (component added, replaced, removed or mutably borrowed), in id order.
    pub changed: Vec<EntityId>,
    /// Entities despawned, oldest first.
    pub despawned: Vec<EntityId>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for World {
    /// Deep copy (used for play worlds). The copy gets its own identity for query caches.
    fn clone(&self) -> Self {
        Self {
            uid: next_uid(),
            entities: self.entities.clone(),
            archetypes: self.archetypes.clone(),
            components: self.components.clone(),
            resources: self.resources.clone(),
            changes: self.changes.clone(),
        }
    }
}

fn next_uid() -> u64 {
    NEXT_WORLD.fetch_add(1, Ordering::Relaxed)
}

/// Builder returned by spawning, for chaining inserts.
pub struct EntityWorldMut<'w> {
    world: &'w mut World,
    id: EntityId,
}

impl std::fmt::Debug for EntityWorldMut<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityWorldMut")
            .field("id", &self.id)
            .finish()
    }
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("entities", &self.len())
            .field("archetypes", &self.archetype_count())
            .finish_non_exhaustive()
    }
}

impl EntityWorldMut<'_> {
    /// The entity's id.
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Inserts a component on this entity.
    pub fn insert<C: Component>(&mut self, component: C) -> Result<&mut Self, EcsError> {
        self.world.insert(self.id, component)?;
        Ok(self)
    }
}

impl World {
    /// An empty world.
    pub fn new() -> Self {
        Self {
            uid: next_uid(),
            entities: EntityTable::default(),
            archetypes: Archetypes::new(),
            components: ComponentIndex::default(),
            resources: Resources::default(),
            changes: ChangeLog::default(),
        }
    }

    /// Despawns remembered for [`World::changes_since`].
    pub const DESPAWN_HISTORY: usize = 4096;

    /// This world's identity (a clone gets a new one), for caches keyed by world.
    pub fn identity(&self) -> u64 {
        self.uid
    }

    /// The current change tick: pass it to [`World::changes_since`] later.
    pub fn change_tick(&self) -> u64 {
        self.changes.tick
    }

    /// What changed after `tick`; `None` if `tick` predates the despawn history (re-read everything).
    pub fn changes_since(&self, tick: u64) -> Option<Changes> {
        if tick < self.changes.floor {
            return None;
        }
        Some(Changes {
            changed: self.entities.changed_since(tick),
            despawned: self
                .changes
                .despawned
                .iter()
                .filter(|(t, _)| *t > tick)
                .map(|(_, id)| *id)
                .collect(),
        })
    }

    /// Marks `id` changed now.
    pub(crate) fn touch(&mut self, id: EntityId) {
        let tick = self.changes.bump();
        self.entities.touch(id, tick);
    }

    /// Number of entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// True if there are no entities.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True if `id` is alive.
    pub fn contains(&self, id: EntityId) -> bool {
        self.entities.contains(id)
    }

    /// The fast handle for `id`.
    pub fn entity(&self, id: EntityId) -> Option<Entity> {
        self.entities.handle(id)
    }

    /// The id behind a handle, or `None` if the handle is stale.
    pub fn id_of(&self, entity: Entity) -> Option<EntityId> {
        self.entities.id_of(entity)
    }

    /// All entity ids, ascending.
    pub fn entity_ids_sorted(&self) -> Vec<EntityId> {
        self.entities.ids().collect()
    }

    /// Number of archetypes (distinct component sets ever seen, including the empty one).
    pub fn archetype_count(&self) -> usize {
        self.archetypes.list.len()
    }

    /// Spawns an entity with a fresh id from `ids`.
    pub fn spawn(&mut self, ids: &mut impl IdSource) -> Result<EntityId, EcsError> {
        let id = ids.next_id()?;
        self.spawn_with_id(id)?;
        Ok(id)
    }

    /// Spawns an entity with no components and the given id.
    pub fn spawn_with_id(&mut self, id: EntityId) -> Result<EntityWorldMut<'_>, EcsError> {
        if self.contains(id) {
            return Err(EcsError::DuplicateId(id));
        }
        let table = &mut self.archetypes.list[Archetypes::EMPTY].table;
        let location = Location {
            archetype: Archetypes::EMPTY,
            row: table.ids.len(),
        };
        self.entities.alloc(id, location)?;
        table.ids.push(id);
        self.touch(id);
        Ok(EntityWorldMut { world: self, id })
    }

    /// Despawns `id`, dropping its components.
    pub fn despawn(&mut self, id: EntityId) -> Result<(), EcsError> {
        let loc = self.entities.location(id)?;
        let swapped = self.archetypes.list[loc.archetype]
            .table
            .swap_remove_drop(loc.row);
        if let Some(moved) = swapped {
            self.entities.set_location(moved, loc);
        }
        self.entities.free(id);
        let tick = self.changes.bump();
        let log = &mut self.changes;
        log.despawned.push_back((tick, id));
        if log.despawned.len() > Self::DESPAWN_HISTORY
            && let Some((dropped, _)) = log.despawned.pop_front()
        {
            log.floor = dropped;
        }
        Ok(())
    }

    /// Inserts or replaces a component; returns the previous value.
    pub fn insert<C: Component>(
        &mut self,
        id: EntityId,
        component: C,
    ) -> Result<Option<C>, EcsError> {
        let loc = self.entities.location(id)?;
        let cid = self.components.register::<C>()?;
        self.touch(id);
        if let Some(col) = self.archetypes.list[loc.archetype].column_index(cid) {
            let column = &mut *self.archetypes.list[loc.archetype].table.columns[col];
            return Ok(Some(std::mem::replace(
                &mut typed_mut::<C>(column)[loc.row],
                component,
            )));
        }
        let dst = self
            .archetypes
            .with_added(loc.archetype, cid, &self.components);
        let new_loc = self.relocate(id, loc, dst, None);
        let col = self.archetypes.list[dst]
            .column_index(cid)
            .expect("destination has the component");
        typed_mut::<C>(&mut *self.archetypes.list[dst].table.columns[col]).push(component);
        debug_assert_eq!(new_loc.row + 1, self.archetypes.list[dst].table.ids.len());
        Ok(None)
    }

    /// Removes a component; returns it, or `None` if the entity did not have it.
    pub fn remove<C: Component>(&mut self, id: EntityId) -> Result<Option<C>, EcsError> {
        let Some(cid) = self.components.id_of(TypeId::of::<C>()) else {
            self.entities.location(id)?;
            return Ok(None);
        };
        self.remove_component(id, cid)?
            .map(downcast::<C>)
            .transpose()
    }

    /// Borrows a component.
    pub fn get<C: Component>(&self, id: EntityId) -> Option<&C> {
        let (loc, col) = self.column_of(id, TypeId::of::<C>())?;
        typed::<C>(&*self.archetypes.list[loc.archetype].table.columns[col]).get(loc.row)
    }

    /// Mutably borrows a component.
    pub fn get_mut<C: Component>(&mut self, id: EntityId) -> Option<&mut C> {
        let (loc, col) = self.column_of(id, TypeId::of::<C>())?;
        self.touch(id);
        typed_mut::<C>(&mut *self.archetypes.list[loc.archetype].table.columns[col])
            .get_mut(loc.row)
    }

    /// Stable names of `id`'s components, sorted; `None` if `id` is not alive.
    pub fn component_names(&self, id: EntityId) -> Option<Vec<&'static str>> {
        let loc = self.entities.location(id).ok()?;
        let arch = &self.archetypes.list[loc.archetype];
        Some(
            arch.by_name
                .iter()
                .map(|&i| self.components.info(arch.components[i]).name)
                .collect(),
        )
    }

    /// A typed query over entities with all of `D`'s components.
    pub fn query<D: QueryData>(&self) -> Result<QueryState<D>, QueryError> {
        QueryState::new()
    }

    /// A typed query with a `With`/`Without` filter.
    pub fn query_filtered<D: QueryData, F: QueryFilter>(
        &self,
    ) -> Result<QueryState<D, F>, QueryError> {
        QueryState::new()
    }

    /// Inserts a resource; returns the previous value.
    pub fn insert_resource<R: Resource + Clone>(&mut self, value: R) -> Option<R> {
        self.resources.insert(value)
    }

    /// Borrows a resource.
    pub fn resource<R: Resource>(&self) -> Option<&R> {
        self.resources.get()
    }

    /// Mutably borrows a resource.
    pub fn resource_mut<R: Resource>(&mut self) -> Option<&mut R> {
        self.resources.get_mut()
    }

    /// Removes a resource.
    pub fn remove_resource<R: Resource>(&mut self) -> Option<R> {
        self.resources.remove()
    }

    /// Takes resource `R` out, runs `f` with the world and the resource, then puts it back.
    pub fn resource_scope<R: Resource + Clone, T>(
        &mut self,
        f: impl FnOnce(&mut World, &mut R) -> T,
    ) -> Option<T> {
        let mut resource = self.remove_resource::<R>()?;
        let out = f(self, &mut resource);
        self.insert_resource(resource);
        Some(out)
    }

    /// Checks storage invariants (column lengths, locations); for tests and debugging.
    pub fn validate_storage(&self) -> Result<(), String> {
        let mut rows = 0;
        for (a, arch) in self.archetypes.list.iter().enumerate() {
            let n = arch.table.ids.len();
            rows += n;
            if let Some(bad) = arch.table.columns.iter().position(|c| c.len() != n) {
                return Err(format!(
                    "archetype {a}: column {bad} has {} rows, ids has {n}",
                    arch.table.columns[bad].len()
                ));
            }
            for (row, &id) in arch.table.ids.iter().enumerate() {
                let loc = self.entities.location(id).map_err(|e| e.to_string())?;
                if loc != (Location { archetype: a, row }) {
                    return Err(format!("{id}: recorded at {loc:?}, stored at ({a}, {row})"));
                }
            }
        }
        match rows == self.len() {
            true => Ok(()),
            false => Err(format!("{rows} stored rows but {} entities", self.len())),
        }
    }

    pub(crate) fn remove_component(
        &mut self,
        id: EntityId,
        cid: ComponentId,
    ) -> Result<Option<Box<dyn DynReflect>>, EcsError> {
        let loc = self.entities.location(id)?;
        self.touch(id);
        let Some(col) = self.archetypes.list[loc.archetype].column_index(cid) else {
            return Ok(None);
        };
        let value =
            self.archetypes.list[loc.archetype].table.columns[col].swap_remove_boxed(loc.row);
        let dst = self
            .archetypes
            .with_removed(loc.archetype, cid, &self.components);
        self.relocate(id, loc, dst, Some(col));
        Ok(Some(value))
    }

    /// Moves `id` from `loc` to archetype `dst`, fixing up the entity swapped into its old row.
    fn relocate(
        &mut self,
        id: EntityId,
        loc: Location,
        dst: usize,
        skip: Option<usize>,
    ) -> Location {
        let (swapped, row) = self.archetypes.move_row(loc.archetype, loc.row, dst, skip);
        let new_loc = Location {
            archetype: dst,
            row,
        };
        self.entities.set_location(id, new_loc);
        if let Some(moved) = swapped {
            self.entities.set_location(moved, loc);
        }
        new_loc
    }

    pub(crate) fn column_of(&self, id: EntityId, type_id: TypeId) -> Option<(Location, usize)> {
        let loc = self.entities.location(id).ok()?;
        let cid = self.components.id_of(type_id)?;
        Some((loc, self.archetypes.list[loc.archetype].column_index(cid)?))
    }
}
