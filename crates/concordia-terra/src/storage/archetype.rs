//! Archetypes (one per component set and shared values) and the transition graph between them.

use std::any::TypeId;
use std::collections::HashMap;

use concordia_core::id::EntityId;

use super::chunk::ChunkColumn;
use super::kind::CHUNK_ROWS;
use super::table::Table;
use crate::component::{ComponentId, ComponentIndex};

/// Where a component of an archetype lives, in name order (for hashing and listing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    /// Dense: column `i`.
    Column(usize),
    /// Shared or tag: entry `i` of `shared`.
    Shared(usize),
}

/// Shared and tag components with their value indices, sorted by component id.
pub(crate) type SharedKey = Vec<(ComponentId, u32)>;

/// Entities sharing exactly the same component set and shared values.
#[derive(Clone)]
pub(crate) struct Archetype {
    /// Sorted dense component ids; column `i` holds `components[i]`.
    pub(crate) components: Vec<ComponentId>,
    /// Shared and tag components, with the index of their one value.
    pub(crate) shared: SharedKey,
    pub(crate) table: Table,
    /// Every component in stable-name order (for hashing and listing).
    pub(crate) by_name: Vec<Slot>,
    /// Change tick of the last write that touched every row (one tick per bulk write).
    pub(crate) changed: u64,
    /// Chunk components by type; value `k` belongs to rows `k * CHUNK_ROWS ..`.
    pub(crate) chunks: Vec<(TypeId, Box<dyn ChunkColumn>)>,
    add_edges: HashMap<ComponentId, usize>,
    remove_edges: HashMap<ComponentId, usize>,
    shared_edges: HashMap<(ComponentId, u32), usize>,
}

impl Archetype {
    fn new(components: Vec<ComponentId>, shared: SharedKey, index: &ComponentIndex) -> Self {
        let columns = components
            .iter()
            .map(|&c| index.info(c).new_column.make())
            .collect();
        let mut by_name: Vec<(&'static str, Slot)> = components
            .iter()
            .enumerate()
            .map(|(i, &c)| (index.info(c).name, Slot::Column(i)))
            .chain(
                shared
                    .iter()
                    .enumerate()
                    .map(|(i, (c, _))| (index.info(*c).name, Slot::Shared(i))),
            )
            .collect();
        by_name.sort_by_key(|(name, _)| *name);
        Self {
            components,
            shared,
            table: Table {
                ids: Vec::new(),
                columns,
            },
            by_name: by_name.into_iter().map(|(_, slot)| slot).collect(),
            changed: 0,
            chunks: Vec::new(),
            add_edges: HashMap::new(),
            remove_edges: HashMap::new(),
            shared_edges: HashMap::new(),
        }
    }

    /// Column index of dense `component`, if this archetype has it.
    pub(crate) fn column_index(&self, component: ComponentId) -> Option<usize> {
        self.components.binary_search(&component).ok()
    }

    /// The value index of shared or tag `component`, if this archetype has it.
    pub(crate) fn shared_value(&self, component: ComponentId) -> Option<u32> {
        self.shared
            .binary_search_by_key(&component, |(c, _)| *c)
            .ok()
            .map(|i| self.shared[i].1)
    }

    /// True if entities here have `component`, in any storage kind.
    pub(crate) fn has(&self, component: ComponentId) -> bool {
        self.column_index(component).is_some() || self.shared_value(component).is_some()
    }

    /// The component a name-order slot holds.
    pub(crate) fn component_at(&self, slot: Slot) -> ComponentId {
        match slot {
            Slot::Column(i) => self.components[i],
            Slot::Shared(i) => self.shared[i].0,
        }
    }

    /// Every component id (dense, then shared and tag).
    pub(crate) fn all_components(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.components
            .iter()
            .copied()
            .chain(self.shared.iter().map(|(c, _)| *c))
    }

    /// Drops chunk values of chunks the table no longer reaches.
    fn trim_chunks(&mut self) {
        let chunks = self.table.ids.len().div_ceil(CHUNK_ROWS);
        for (_, column) in &mut self.chunks {
            column.truncate(chunks);
        }
    }
}

/// All archetypes; index 0 is the empty archetype.
#[derive(Clone)]
pub(crate) struct Archetypes {
    pub(crate) list: Vec<Archetype>,
    by_key: HashMap<(Vec<ComponentId>, SharedKey), usize>,
}

impl Archetypes {
    pub(crate) const EMPTY: usize = 0;

    pub(crate) fn new() -> Self {
        let empty = Archetype::new(Vec::new(), Vec::new(), &ComponentIndex::default());
        Self {
            list: vec![empty],
            by_key: HashMap::from([((Vec::new(), Vec::new()), Self::EMPTY)]),
        }
    }

    fn get_or_create(
        &mut self,
        components: Vec<ComponentId>,
        shared: SharedKey,
        index: &ComponentIndex,
    ) -> usize {
        let key = (components, shared);
        if let Some(&existing) = self.by_key.get(&key) {
            return existing;
        }
        let id = self.list.len();
        self.list
            .push(Archetype::new(key.0.clone(), key.1.clone(), index));
        self.by_key.insert(key, id);
        id
    }

    /// The archetype reached by adding dense `component` to `src`.
    pub(crate) fn with_added(
        &mut self,
        src: usize,
        component: ComponentId,
        index: &ComponentIndex,
    ) -> usize {
        if let Some(&dst) = self.list[src].add_edges.get(&component) {
            return dst;
        }
        let mut components = self.list[src].components.clone();
        if let Err(pos) = components.binary_search(&component) {
            components.insert(pos, component);
        }
        let shared = self.list[src].shared.clone();
        let dst = self.get_or_create(components, shared, index);
        self.list[src].add_edges.insert(component, dst);
        dst
    }

    /// The archetype reached by removing `component` (dense, shared or tag) from `src`.
    pub(crate) fn with_removed(
        &mut self,
        src: usize,
        component: ComponentId,
        index: &ComponentIndex,
    ) -> usize {
        if let Some(&dst) = self.list[src].remove_edges.get(&component) {
            return dst;
        }
        let mut components = self.list[src].components.clone();
        components.retain(|&c| c != component);
        let mut shared = self.list[src].shared.clone();
        shared.retain(|(c, _)| *c != component);
        let dst = self.get_or_create(components, shared, index);
        self.list[src].remove_edges.insert(component, dst);
        dst
    }

    /// The archetype reached by setting shared or tag `component` to value `value` in `src`.
    pub(crate) fn with_shared(
        &mut self,
        src: usize,
        component: ComponentId,
        value: u32,
        index: &ComponentIndex,
    ) -> usize {
        if let Some(&dst) = self.list[src].shared_edges.get(&(component, value)) {
            return dst;
        }
        let components = self.list[src].components.clone();
        let mut shared = self.list[src].shared.clone();
        match shared.binary_search_by_key(&component, |(c, _)| *c) {
            Ok(i) => shared[i].1 = value,
            Err(i) => shared.insert(i, (component, value)),
        }
        let dst = self.get_or_create(components, shared, index);
        self.list[src].shared_edges.insert((component, value), dst);
        dst
    }

    /// Moves `row` of `src` to the end of `dst`.
    ///
    /// Column `skip` (already removed by the caller) is left alone; columns
    /// `dst` lacks are dropped. Returns the id swapped into `row` (if any)
    /// and the new row in `dst`.
    pub(crate) fn move_row(
        &mut self,
        src: usize,
        row: usize,
        dst: usize,
        skip: Option<usize>,
    ) -> (Option<EntityId>, usize) {
        let [from, to] = self
            .list
            .get_disjoint_mut([src, dst])
            .expect("source and destination archetypes differ");
        for (i, component) in from.components.iter().enumerate() {
            if Some(i) == skip {
                continue;
            }
            let column = &mut from.table.columns[i];
            match to.column_index(*component) {
                Some(j) => column.swap_remove_into(row, &mut *to.table.columns[j]),
                None => column.swap_remove_drop(row),
            }
        }
        let id = from.table.ids.swap_remove(row);
        to.table.ids.push(id);
        from.trim_chunks();
        (from.table.ids.get(row).copied(), to.table.ids.len() - 1)
    }

    /// Removes `row` of archetype `a` (dropping its values); returns the id swapped into `row`.
    pub(crate) fn remove_row(&mut self, a: usize, row: usize) -> Option<EntityId> {
        let arch = &mut self.list[a];
        let swapped = arch.table.swap_remove_drop(row);
        arch.trim_chunks();
        swapped
    }
}
