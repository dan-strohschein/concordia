//! The crowd's spatial grid: a snapshot of every crowd agent and every other character at the
//! start of the crowd's step (last tick's state), bucketed into square cells. A resource rebuilt
//! each tick from hashed state, so it isn't hashed.

use std::collections::HashMap;

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;

/// One agent or character in the snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridEntry {
    /// The entity.
    pub id: EntityId,
    /// Its feet.
    pub position: Vec3,
    /// Its radius.
    pub radius: f32,
    /// Its height.
    pub height: f32,
    /// Its velocity.
    pub velocity: Vec3,
    /// The velocity it wants.
    pub desired_velocity: Vec3,
    /// It has a body physics moves (a `CharacterController`): surface agents never push it.
    pub solid: bool,
    /// It is a crowd agent.
    pub crowd: bool,
}

/// Every agent and character by cell, in id order within a cell.
#[derive(Debug, Clone, Default)]
pub struct CrowdGrid {
    cell: f32,
    entries: Vec<GridEntry>,
    index: HashMap<EntityId, usize>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl CrowdGrid {
    /// A grid of `entries` (sorted by id here) in cells `cell` meters wide (at least 0.01).
    pub fn new(mut entries: Vec<GridEntry>, cell: f32) -> Self {
        entries.sort_by_key(|e| e.id);
        let cell = if cell.is_finite() {
            cell.max(0.01)
        } else {
            1.0
        };
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        let mut index = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            index.insert(e.id, i);
            cells
                .entry(cell_of(e.position, cell))
                .or_default()
                .push(i as u32);
        }
        Self {
            cell,
            entries,
            index,
            cells,
        }
    }

    /// The entry of `id`.
    pub fn get(&self, id: EntityId) -> Option<&GridEntry> {
        self.index.get(&id).map(|&i| &self.entries[i])
    }

    /// Every entry, by id.
    pub fn entries(&self) -> &[GridEntry] {
        &self.entries
    }

    /// Entries whose cell overlaps the square `center ± range` (a superset of those within range).
    pub fn around(&self, center: Vec3, range: f32) -> impl Iterator<Item = &GridEntry> {
        let (x0, z0) = cell_of(center - Vec3::new(range, 0.0, range), self.cell);
        let (x1, z1) = cell_of(center + Vec3::new(range, 0.0, range), self.cell);
        (z0..=z1)
            .flat_map(move |z| (x0..=x1).map(move |x| (x, z)))
            .filter_map(|key| self.cells.get(&key))
            .flatten()
            .map(|&i| &self.entries[i as usize])
    }
}

/// The cell holding `p` sideways.
fn cell_of(p: Vec3, cell: f32) -> (i32, i32) {
    ((p.x / cell).floor() as i32, (p.z / cell).floor() as i32)
}
