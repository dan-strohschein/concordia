//! Typed queries over archetypes, sequential and parallel.

mod data;
mod filter;

use std::any::TypeId;
use std::marker::PhantomData;
use std::num::NonZeroUsize;

use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;

pub use data::{ElemAccess, QueryData, QueryElem};
pub(crate) use data::{ViewCol, ViewColumn};
pub use filter::{QueryFilter, With, Without};

use crate::error::QueryError;
use crate::storage::archetype::Archetype;
use crate::world::World;
use data::TableAccess;

const ONE: NonZeroUsize = NonZeroUsize::MIN;

/// Cached matching state for query `D` with filter `F`; new archetypes are picked up on each run.
pub struct QueryState<D: QueryData, F: QueryFilter = ()> {
    elems: Vec<ElemAccess>,
    with: Vec<TypeId>,
    without: Vec<TypeId>,
    world: u64,
    /// Per archetype: the columns for `D`'s elements, if it matches.
    columns: Vec<Option<Vec<usize>>>,
    marker: PhantomData<fn() -> (D, F)>,
}

impl<D: QueryData, F: QueryFilter> QueryState<D, F> {
    /// Builds the query, rejecting repeated components.
    pub fn new() -> Result<Self, QueryError> {
        let elems = D::elems();
        for (i, elem) in elems.iter().enumerate() {
            if elems[..i].iter().any(|e| e.type_id == elem.type_id) {
                return Err(QueryError::Conflict {
                    component: elem.name,
                });
            }
        }
        let (mut with, mut without) = (Vec::new(), Vec::new());
        F::collect(&mut with, &mut without);
        Ok(Self {
            elems,
            with,
            without,
            world: 0,
            columns: Vec::new(),
            marker: PhantomData,
        })
    }

    /// Calls `f` for every matching entity.
    pub fn for_each(&mut self, world: &mut World, mut f: impl FnMut(EntityId, D::Item<'_>)) {
        self.update(world);
        for (arch, cols) in matched(&mut world.archetypes.list, &self.columns) {
            let table = &mut arch.table;
            let mut slices = D::fetch(TableAccess(&mut table.columns), cols);
            for (row, &id) in table.ids.iter().enumerate() {
                f(id, D::get(&mut slices, row));
            }
        }
        self.mark_written(world);
    }

    /// Calls `f` for every matching entity in parallel, in fixed `chunk`-row pieces.
    ///
    /// Chunking depends only on `chunk`, never on the thread count.
    pub fn par_for_each(
        &mut self,
        world: &mut World,
        jobs: &JobSystem,
        chunk: NonZeroUsize,
        f: impl Fn(EntityId, D::Item<'_>) + Sync,
    ) {
        self.update(world);
        let mut work = Vec::new();
        for (arch, cols) in matched(&mut world.archetypes.list, &self.columns) {
            let table = &mut arch.table;
            let slices = D::fetch(TableAccess(&mut table.columns), cols);
            split_chunks::<D>(&table.ids, slices, chunk.get(), &mut work);
        }
        jobs.par_chunks_mut(&mut work, ONE, |_, pieces| {
            for (ids, slices) in pieces.iter_mut() {
                for (row, &id) in ids.iter().enumerate() {
                    f(id, D::get(slices, row));
                }
            }
        });
        drop(work);
        self.mark_written(world);
    }

    /// A query with `&mut` access counts as changing every entity it visited.
    fn mark_written(&self, world: &mut World) {
        if !self.elems.iter().any(|e| e.mutable) {
            return;
        }
        let tick = world.changes.bump();
        for (arch, _) in matched(&mut world.archetypes.list, &self.columns) {
            for &id in &arch.table.ids {
                world.entities.touch(id, tick);
            }
        }
    }

    fn update(&mut self, world: &World) {
        if self.world != world.uid {
            self.world = world.uid;
            self.columns.clear();
        }
        for arch in &world.archetypes.list[self.columns.len()..] {
            let cols = self.match_archetype(arch, world);
            self.columns.push(cols);
        }
    }

    fn match_archetype(&self, arch: &Archetype, world: &World) -> Option<Vec<usize>> {
        let has = |t: &TypeId| {
            world
                .components
                .id_of(*t)
                .and_then(|c| arch.column_index(c))
                .is_some()
        };
        if !self.with.iter().all(has) || self.without.iter().any(has) {
            return None;
        }
        self.elems
            .iter()
            .map(|e| {
                world
                    .components
                    .id_of(e.type_id)
                    .and_then(|c| arch.column_index(c))
            })
            .collect()
    }
}

fn matched<'a>(
    list: &'a mut [Archetype],
    columns: &'a [Option<Vec<usize>>],
) -> impl Iterator<Item = (&'a mut Archetype, &'a [usize])> {
    list.iter_mut()
        .zip(columns)
        .filter_map(|(arch, cols)| cols.as_deref().map(|c| (arch, c)))
        .filter(|(arch, _)| !arch.table.ids.is_empty())
}

pub(crate) type Piece<'a, D> = (&'a [EntityId], <D as QueryData>::Slices<'a>);

pub(crate) fn split_chunks<'a, D: QueryData>(
    mut ids: &'a [EntityId],
    mut slices: D::Slices<'a>,
    chunk: usize,
    out: &mut Vec<Piece<'a, D>>,
) {
    while ids.len() > chunk {
        let (head, tail) = ids.split_at(chunk);
        let (head_slices, tail_slices) = D::split_at(slices, chunk);
        out.push((head, head_slices));
        ids = tail;
        slices = tail_slices;
    }
    out.push((ids, slices));
}
