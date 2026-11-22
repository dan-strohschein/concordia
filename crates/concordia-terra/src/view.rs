//! A processor's view of the world: only its declared columns, resources and signals, plus its
//! command buffer (ADR-0063, ADR-0069).

use std::any::TypeId;
use std::num::NonZeroUsize;

use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;

use crate::command::CommandBuffer;
use crate::component::Component;
use crate::entity::EntityTable;
use crate::error::QueryError;
use crate::lod::{Due, SimLod};
use crate::processor::{Access, Phase};
use crate::query::{ElemAccess, Piece, QueryData, ViewCol, ViewColumn, split_chunks};
use crate::resource::Resource;
use crate::signal::SignalQueue;
use crate::storage::column::typed;

/// One archetype as a view sees it: its ids and the declared columns it has.
pub(crate) struct ViewArch<'w> {
    pub(crate) index: usize,
    pub(crate) ids: &'w [EntityId],
    pub(crate) columns: Vec<(&'static str, ViewCol<'w>)>,
}

/// A lent resource: shared for readers, exclusive for the one writer.
pub(crate) enum ViewRes<'w> {
    Read(&'w dyn Resource),
    Write(&'w mut dyn Resource),
}

/// A processor's restricted view of the world: only what it declared.
pub struct View<'w> {
    pub(crate) name: &'w str,
    pub(crate) phase: Phase,
    pub(crate) access: &'w Access,
    pub(crate) archetypes: Vec<ViewArch<'w>>,
    pub(crate) entities: &'w EntityTable,
    pub(crate) signals: &'w SignalQueue,
    pub(crate) resources: Vec<(TypeId, ViewRes<'w>)>,
    /// For an observer: the entities that triggered it.
    pub(crate) observed: Vec<EntityId>,
    /// This tick's step, handed to entities without a `SimLod` (and to every entity of a
    /// processor that isn't LOD-aware).
    pub(crate) step: Due,
    /// Archetypes each mutable query visited (their entities count as changed).
    pub(crate) touched: Vec<Vec<usize>>,
    /// Structural changes, applied at the end of the phase.
    pub(crate) commands: CommandBuffer,
}

impl<'w> View<'w> {
    /// A query over the declared columns; an undeclared component, or a write of one declared
    /// read-only, is an error.
    pub fn query<D: QueryData>(&mut self) -> Result<ViewQuery<'_, 'w, D>, QueryError> {
        let elems = D::elems();
        for (i, elem) in elems.iter().enumerate() {
            if elems[..i].iter().any(|e| e.name == elem.name) {
                return Err(QueryError::Conflict {
                    component: elem.name,
                });
            }
            if !self.access.touches(elem.name) {
                return Err(QueryError::Undeclared {
                    component: elem.name,
                });
            }
            if elem.mutable && !self.access.writes(elem.name) {
                return Err(QueryError::ReadOnly {
                    component: elem.name,
                });
            }
            if elem.mutable && self.is_shared(elem.name) {
                return Err(QueryError::SharedReadOnly {
                    component: elem.name,
                });
            }
        }
        Ok(ViewQuery {
            view: self,
            elems,
            marker: std::marker::PhantomData,
        })
    }

    pub(crate) fn is_shared(&self, name: &str) -> bool {
        self.archetypes.iter().any(|a| {
            a.columns
                .iter()
                .any(|(n, c)| *n == name && matches!(c, ViewCol::Shared(_)))
        })
    }

    /// This processor's command buffer, applied at the end of its phase in plan order.
    pub fn commands(&mut self) -> Result<&mut CommandBuffer, QueryError> {
        if self.phase == Phase::Presentation {
            return Err(QueryError::CommandsInPresentation {
                processor: self.name.to_owned(),
            });
        }
        Ok(&mut self.commands)
    }

    /// This tick's delivered `S` signals, by entity then raise order (declare with `receives`).
    pub fn signals<S: Component>(&self) -> Result<&[(EntityId, S)], QueryError> {
        let name = S::type_info().name;
        if !self.access.receives(name) {
            return Err(QueryError::Undeclared { component: name });
        }
        Ok(self.signals.delivered::<S>())
    }

    /// Declared resource `R`.
    pub fn resource<R: Resource>(&self) -> Result<&R, QueryError> {
        let name = std::any::type_name::<R>();
        if !self
            .access
            .resources()
            .any(|(k, _)| k.type_id == TypeId::of::<R>())
        {
            return Err(QueryError::Undeclared { component: name });
        }
        let lent = self.resources.iter().find(|(t, _)| *t == TypeId::of::<R>());
        let found = lent.and_then(|(_, r)| match r {
            // Deref to `dyn Resource`: references would pick the blanket impl.
            ViewRes::Read(r) => (**r).as_any().downcast_ref::<R>(),
            ViewRes::Write(r) => (**r).as_any().downcast_ref::<R>(),
        });
        found.ok_or(QueryError::MissingResource { resource: name })
    }

    /// Declared writable resource `R`.
    pub fn resource_mut<R: Resource>(&mut self) -> Result<&mut R, QueryError> {
        let name = std::any::type_name::<R>();
        match self
            .access
            .resources()
            .find(|(k, _)| k.type_id == TypeId::of::<R>())
        {
            None => return Err(QueryError::Undeclared { component: name }),
            Some((_, false)) => return Err(QueryError::ReadOnly { component: name }),
            Some((_, true)) => {}
        }
        let lent = self
            .resources
            .iter_mut()
            .find(|(t, _)| *t == TypeId::of::<R>());
        let found = lent.and_then(|(_, r)| match r {
            ViewRes::Write(r) => (**r).as_any_mut().downcast_mut::<R>(),
            ViewRes::Read(_) => None,
        });
        found.ok_or(QueryError::MissingResource { resource: name })
    }

    /// For an observer: the entities that gained or lost its component, in id order.
    pub fn observed(&self) -> &[EntityId] {
        &self.observed
    }
}

/// A query over a [`View`]; see [`View::query`].
pub struct ViewQuery<'v, 'w, D: QueryData> {
    view: &'v mut View<'w>,
    elems: Vec<ElemAccess>,
    marker: std::marker::PhantomData<fn() -> D>,
}

/// One piece's rows and, for a LOD-aware view, their `SimLod`s (`None`: every row is due).
type Rows<'v, D> = (Piece<'v, D>, Option<&'v [SimLod]>);

/// One archetype's matching slices.
struct Matched<'v, D: QueryData> {
    archetype: usize,
    pieces: Vec<Rows<'v, D>>,
}

/// What a view query borrows to run.
struct Run<'v, D: QueryData> {
    matched: Vec<Matched<'v, D>>,
    touched: Touched<'v>,
    commands: &'v mut CommandBuffer,
    entities: &'v EntityTable,
    step: Due,
}

impl<'v, D: QueryData> Run<'v, D> {
    /// Every piece, in archetype then row order.
    fn pieces(&mut self) -> Vec<Rows<'v, D>> {
        std::mem::take(&mut self.matched)
            .into_iter()
            .flat_map(|m| m.pieces)
            .collect()
    }
}

/// Calls `f` for each due row of one piece (every row without `SimLod`s).
fn visit<D: QueryData>(
    (ids, slices): &mut Piece<'_, D>,
    lod: Option<&[SimLod]>,
    step: Due,
    mut f: impl FnMut(EntityId, Due, D::Item<'_>),
) {
    for (row, &id) in ids.iter().enumerate() {
        let due = match lod.map(|rows| &rows[row]) {
            None => step,
            Some(l) if !l.due => continue,
            Some(l) => Due {
                dt: l.elapsed as f32,
                dt_exact: l.elapsed,
                level: l.level,
            },
        };
        f(id, due, D::get(slices, row));
    }
}

impl<'v, D: QueryData> ViewQuery<'v, '_, D> {
    /// Calls `f` for every matching entity (for a LOD-aware processor, every one due this tick).
    pub fn for_each(self, mut f: impl FnMut(EntityId, D::Item<'_>)) {
        self.for_each_due(|id, _, item| f(id, item));
    }

    /// Like [`ViewQuery::for_each`], handing each entity its [`Due`] step.
    pub fn for_each_due(self, mut f: impl FnMut(EntityId, Due, D::Item<'_>)) {
        let mut run = self.run(usize::MAX);
        let step = run.step;
        for (mut piece, lod) in run.pieces() {
            visit::<D>(&mut piece, lod, step, &mut f);
        }
        run.touched.record();
    }

    /// Calls `f` for every matching (and, if LOD-aware, due) entity in parallel, in fixed
    /// `chunk`-row pieces (chunking depends only on `chunk`, never on the thread count or LOD).
    pub fn par_for_each(
        self,
        jobs: &JobSystem,
        chunk: NonZeroUsize,
        f: impl Fn(EntityId, D::Item<'_>) + Sync,
    ) {
        self.par_for_each_due(jobs, chunk, |id, _, item| f(id, item));
    }

    /// Like [`ViewQuery::par_for_each`], handing each entity its [`Due`] step.
    pub fn par_for_each_due(
        self,
        jobs: &JobSystem,
        chunk: NonZeroUsize,
        f: impl Fn(EntityId, Due, D::Item<'_>) + Sync,
    ) {
        let mut run = self.run(chunk.get());
        let step = run.step;
        let mut pieces = run.pieces();
        jobs.par_chunks_mut(&mut pieces, NonZeroUsize::MIN, |_, work| {
            for (piece, lod) in work.iter_mut() {
                visit::<D>(piece, *lod, step, &f);
            }
        });
        drop(pieces);
        run.touched.record();
    }

    /// Like [`ViewQuery::par_for_each`], giving each piece its own command buffer; the buffers
    /// join this processor's in piece order, so the result never depends on the thread count.
    pub fn par_for_each_deferred(
        self,
        jobs: &JobSystem,
        chunk: NonZeroUsize,
        f: impl Fn(&mut CommandBuffer, EntityId, D::Item<'_>) + Sync,
    ) {
        self.par_for_each_deferred_due(jobs, chunk, |c, id, _, item| f(c, id, item));
    }

    /// Like [`ViewQuery::par_for_each_deferred`], handing each entity its [`Due`] step.
    pub fn par_for_each_deferred_due(
        self,
        jobs: &JobSystem,
        chunk: NonZeroUsize,
        f: impl Fn(&mut CommandBuffer, EntityId, Due, D::Item<'_>) + Sync,
    ) {
        let mut run = self.run(chunk.get());
        let step = run.step;
        let mut work: Vec<_> = run
            .pieces()
            .into_iter()
            .map(|rows| (rows, CommandBuffer::new()))
            .collect();
        jobs.par_chunks_mut(&mut work, NonZeroUsize::MIN, |_, items| {
            for ((piece, lod), commands) in items.iter_mut() {
                visit::<D>(piece, *lod, step, |id, due, item| {
                    f(commands, id, due, item)
                });
            }
        });
        for (_, commands) in work {
            run.commands.append(commands);
        }
        run.touched.record();
    }

    /// Calls `f` for each of `ids` that is alive and matches, in the order given, whatever its
    /// LOD (signal targets are always reachable, ADR-0076).
    pub fn for_each_of(self, ids: &[EntityId], mut f: impl FnMut(EntityId, D::Item<'_>)) {
        let run = self.run(usize::MAX);
        let mut matched = run.matched;
        for &id in ids {
            let Ok(loc) = run.entities.location(id) else {
                continue;
            };
            let found = matched.iter_mut().find(|m| m.archetype == loc.archetype);
            if let Some(((_, slices), _)) = found.and_then(|m| m.pieces.first_mut()) {
                f(id, D::get(slices, loc.row));
            }
        }
        run.touched.record();
    }

    /// The matching archetypes' slices, split into `chunk`-row pieces, with their `SimLod`s if
    /// the view is LOD-aware.
    fn run(self, chunk: usize) -> Run<'v, D> {
        let mutable = self.elems.iter().any(|e| e.mutable);
        let elems = self.elems;
        let View {
            archetypes,
            touched,
            commands,
            entities,
            access,
            step,
            ..
        } = self.view;
        let lod_aware = access.lod;
        let mut visited = Vec::new();
        let mut matched = Vec::new();
        for arch in archetypes.iter_mut().filter(|a| !a.ids.is_empty()) {
            let lod = if lod_aware { sim_lods(arch) } else { None };
            let mut picked: Vec<Option<ViewColumn<'_>>> = (0..elems.len()).map(|_| None).collect();
            for (name, column) in arch.columns.iter_mut() {
                if let Some(k) = elems.iter().position(|e| e.name == *name) {
                    picked[k] = Some(ViewColumn(match column {
                        ViewCol::Read(c) => ViewCol::Read(*c),
                        ViewCol::Write(c) => ViewCol::Write(&mut **c),
                        ViewCol::Shared(s) => ViewCol::Shared(*s),
                    }));
                }
            }
            let Some(columns) = picked.into_iter().collect::<Option<Vec<_>>>() else {
                continue;
            };
            visited.push(arch.index);
            let mut pieces = Vec::new();
            split_chunks::<D>(arch.ids, D::fetch_view(columns), chunk, &mut pieces);
            let lods = split_lods(lod, chunk, pieces.len());
            matched.push(Matched {
                archetype: arch.index,
                pieces: pieces.into_iter().zip(lods).collect(),
            });
        }
        let record = if mutable { Some(visited) } else { None };
        Run {
            matched,
            touched: Touched { touched, record },
            commands,
            entities,
            step: *step,
        }
    }
}

/// An archetype's `SimLod` rows, if it has them (a LOD-aware view always reads them).
fn sim_lods<'w>(arch: &ViewArch<'w>) -> Option<&'w [SimLod]> {
    let name = <SimLod as concordia_reflect::Reflect>::type_info().name;
    arch.columns.iter().find_map(|(n, c)| match c {
        ViewCol::Read(c) if *n == name => Some(typed::<SimLod>(*c).as_slice()),
        _ => None,
    })
}

/// `lod` split like [`split_chunks`] splits rows into `pieces` pieces of `chunk` rows.
fn split_lods(lod: Option<&[SimLod]>, chunk: usize, pieces: usize) -> Vec<Option<&[SimLod]>> {
    match lod {
        None => vec![None; pieces],
        Some(rows) => {
            let mut out: Vec<Option<&[SimLod]>> = rows.chunks(chunk.max(1)).map(Some).collect();
            out.resize(pieces, Some(&[]));
            out
        }
    }
}

/// Records a mutable query's visited archetypes once it has run.
struct Touched<'v> {
    touched: &'v mut Vec<Vec<usize>>,
    record: Option<Vec<usize>>,
}

impl Touched<'_> {
    fn record(self) {
        if let Some(visited) = self.record {
            self.touched.push(visited);
        }
    }
}
