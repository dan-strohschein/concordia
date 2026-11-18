//! Rows of entities with one column per component.

use concordia_core::id::EntityId;

use super::column::Column;

/// Entity ids by row, plus one column per component (sorted by component id).
#[derive(Clone, Default)]
pub(crate) struct Table {
    pub(crate) ids: Vec<EntityId>,
    pub(crate) columns: Vec<Box<dyn Column>>,
}

impl Table {
    /// Removes `row` from every column (dropping values) and returns the id now at `row`, if any.
    pub(crate) fn swap_remove_drop(&mut self, row: usize) -> Option<EntityId> {
        debug_assert!(row < self.ids.len(), "row in bounds");
        for column in &mut self.columns {
            column.swap_remove_drop(row);
        }
        self.ids.swap_remove(row);
        self.ids.get(row).copied()
    }
}
