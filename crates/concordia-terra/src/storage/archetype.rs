//! Archetypes (one per component set) and the transition graph between them.

use std::collections::HashMap;

use concordia_core::id::EntityId;

use super::table::Table;
use crate::component::{ComponentId, ComponentIndex};

/// Entities sharing exactly the same component set.
#[derive(Clone)]
pub(crate) struct Archetype {
    /// Sorted component ids; column `i` holds `components[i]`.
    pub(crate) components: Vec<ComponentId>,
    pub(crate) table: Table,
    /// Column indices ordered by component stable name (for hashing and listing).
    pub(crate) by_name: Vec<usize>,
    add_edges: HashMap<ComponentId, usize>,
    remove_edges: HashMap<ComponentId, usize>,
}

impl Archetype {
    fn new(components: Vec<ComponentId>, index: &ComponentIndex) -> Self {
        let columns = components
            .iter()
            .map(|&c| (index.info(c).new_column)())
            .collect();
        let mut by_name: Vec<usize> = (0..components.len()).collect();
        by_name.sort_by_key(|&i| index.info(components[i]).name);
        Self {
            components,
            table: Table {
                ids: Vec::new(),
                columns,
            },
            by_name,
            add_edges: HashMap::new(),
            remove_edges: HashMap::new(),
        }
    }

    /// Column index of `component`, if this archetype has it.
    pub(crate) fn column_index(&self, component: ComponentId) -> Option<usize> {
        self.components.binary_search(&component).ok()
    }
}

/// All archetypes; index 0 is the empty archetype.
#[derive(Clone)]
pub(crate) struct Archetypes {
    pub(crate) list: Vec<Archetype>,
    by_components: HashMap<Vec<ComponentId>, usize>,
}

impl Archetypes {
    pub(crate) const EMPTY: usize = 0;

    pub(crate) fn new() -> Self {
        let empty = Archetype::new(Vec::new(), &ComponentIndex::default());
        Self {
            list: vec![empty],
            by_components: HashMap::from([(Vec::new(), Self::EMPTY)]),
        }
    }

    fn get_or_create(&mut self, components: Vec<ComponentId>, index: &ComponentIndex) -> usize {
        if let Some(&existing) = self.by_components.get(&components) {
            return existing;
        }
        let id = self.list.len();
        self.list.push(Archetype::new(components.clone(), index));
        self.by_components.insert(components, id);
        id
    }

    /// The archetype reached by adding `component` to `src`.
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
        let dst = self.get_or_create(components, index);
        self.list[src].add_edges.insert(component, dst);
        dst
    }

    /// The archetype reached by removing `component` from `src`.
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
        let dst = self.get_or_create(components, index);
        self.list[src].remove_edges.insert(component, dst);
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
        (from.table.ids.get(row).copied(), to.table.ids.len() - 1)
    }
}
