//! Processors (M2 phase 2.0, ADR-0063): systems that declare what they read and write, grouped into
//! processing phases, so the scheduler can run non-conflicting ones in parallel deterministically.
//! Modeled on the processor and processing-phase concepts of Unreal's Mass, from its public
//! documentation only (ADR-0067).

use std::any::TypeId;
use std::num::NonZeroUsize;

use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;

use crate::component::Component;
use crate::error::QueryError;
use crate::query::{ElemAccess, Piece, QueryData, ViewCol, ViewColumn, split_chunks};
use crate::schedule::SystemCtx;

/// Processing phases, in run order each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// Input, intent and anything physics should see this tick.
    PrePhysics,
    /// The physics step.
    Physics,
    /// Gameplay reacting to physics: most game logic.
    PostPhysics,
    /// End-of-tick bookkeeping (cleanup, late writes).
    FrameEnd,
    /// Presentation-only state (representation, audio cues); never changes the world hash.
    Presentation,
}

impl Phase {
    /// Every phase, in run order.
    pub const ALL: [Phase; 5] = [
        Phase::PrePhysics,
        Phase::Physics,
        Phase::PostPhysics,
        Phase::FrameEnd,
        Phase::Presentation,
    ];
}

/// One declared component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    pub(crate) type_id: TypeId,
    pub(crate) name: &'static str,
}

/// What a processor touches.
#[derive(Debug, Clone, Default)]
pub(crate) struct Access {
    pub(crate) reads: Vec<Key>,
    pub(crate) writes: Vec<Key>,
    /// Presentation-only components it writes (never hashed).
    pub(crate) presentation: Vec<Key>,
}

impl Access {
    /// Every component it may write.
    pub(crate) fn written(&self) -> impl Iterator<Item = &Key> {
        self.writes.iter().chain(&self.presentation)
    }

    /// Every component it touches.
    pub(crate) fn touched(&self) -> impl Iterator<Item = &Key> {
        self.reads.iter().chain(self.written())
    }

    fn writes(&self, t: TypeId) -> bool {
        self.written().any(|k| k.type_id == t)
    }

    fn touches(&self, t: TypeId) -> bool {
        self.touched().any(|k| k.type_id == t)
    }

    /// True if running both at once could race: one writes what the other touches.
    pub(crate) fn conflicts(&self, other: &Access) -> bool {
        self.written().any(|k| other.touches(k.type_id))
            || other.written().any(|k| self.touches(k.type_id))
    }
}

/// A processor's name, phase, access and order constraints; built by [`processor`].
#[derive(Debug, Clone)]
pub struct ProcessorDecl {
    pub(crate) name: String,
    pub(crate) phase: Phase,
    pub(crate) access: Access,
    pub(crate) after: Vec<String>,
    pub(crate) before: Vec<String>,
}

/// Starts a processor named `name` in `phase`.
pub fn processor(name: &str, phase: Phase) -> ProcessorDecl {
    ProcessorDecl {
        name: name.to_owned(),
        phase,
        access: Access::default(),
        after: Vec::new(),
        before: Vec::new(),
    }
}

fn key<T: Component>() -> Key {
    Key {
        type_id: TypeId::of::<T>(),
        name: T::type_info().name,
    }
}

impl ProcessorDecl {
    /// Declares a component read.
    #[must_use]
    pub fn reads<T: Component>(mut self) -> Self {
        self.access.reads.push(key::<T>());
        self
    }

    /// Declares a component write.
    #[must_use]
    pub fn writes<T: Component>(mut self) -> Self {
        self.access.writes.push(key::<T>());
        self
    }

    /// Declares a write of a presentation-only component (excluded from the world hash).
    #[must_use]
    pub fn writes_presentation<T: Component>(mut self) -> Self {
        self.access.presentation.push(key::<T>());
        self
    }

    /// Runs after the processor or system `name` (same phase).
    #[must_use]
    pub fn after(mut self, name: &str) -> Self {
        self.after.push(name.to_owned());
        self
    }

    /// Runs before the processor or system `name` (same phase).
    #[must_use]
    pub fn before(mut self, name: &str) -> Self {
        self.before.push(name.to_owned());
        self
    }

    /// The name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Finishes the processor with its body.
    pub fn run<F>(self, f: F) -> FnProcessor<F>
    where
        F: FnMut(&mut View<'_>, &SystemCtx<'_>) + Send,
    {
        FnProcessor { decl: self, f }
    }
}

/// A processor: a named, phased unit of logic with declared access.
pub trait Processor: Send {
    /// Its declaration.
    fn decl(&self) -> &ProcessorDecl;
    /// Runs once per tick, seeing only what it declared.
    fn run(&mut self, view: &mut View<'_>, ctx: &SystemCtx<'_>);
}

/// A processor made from a closure; see [`ProcessorDecl::run`].
pub struct FnProcessor<F> {
    decl: ProcessorDecl,
    f: F,
}

impl<F> Processor for FnProcessor<F>
where
    F: FnMut(&mut View<'_>, &SystemCtx<'_>) + Send,
{
    fn decl(&self) -> &ProcessorDecl {
        &self.decl
    }

    fn run(&mut self, view: &mut View<'_>, ctx: &SystemCtx<'_>) {
        (self.f)(view, ctx);
    }
}

/// One archetype as a view sees it: its ids and the declared columns it has.
pub(crate) struct ViewArch<'w> {
    pub(crate) index: usize,
    pub(crate) ids: &'w [EntityId],
    pub(crate) columns: Vec<(TypeId, ViewCol<'w>)>,
}

/// A processor's restricted view of the world: only the columns it declared.
pub struct View<'w> {
    pub(crate) access: &'w Access,
    pub(crate) archetypes: Vec<ViewArch<'w>>,
    /// Archetypes each mutable query visited (their entities count as changed).
    pub(crate) touched: Vec<Vec<usize>>,
}

impl<'w> View<'w> {
    /// A query over the declared columns; an undeclared component, or a write of one declared
    /// read-only, is an error.
    pub fn query<D: QueryData>(&mut self) -> Result<ViewQuery<'_, 'w, D>, QueryError> {
        let elems = D::elems();
        for (i, elem) in elems.iter().enumerate() {
            if elems[..i].iter().any(|e| e.type_id == elem.type_id) {
                return Err(QueryError::Conflict {
                    component: elem.name,
                });
            }
            if !self.access.touches(elem.type_id) {
                return Err(QueryError::Undeclared {
                    component: elem.name,
                });
            }
            if elem.mutable && !self.access.writes(elem.type_id) {
                return Err(QueryError::ReadOnly {
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
}

/// A query over a [`View`]; see [`View::query`].
pub struct ViewQuery<'v, 'w, D: QueryData> {
    view: &'v mut View<'w>,
    elems: Vec<ElemAccess>,
    marker: std::marker::PhantomData<fn() -> D>,
}

impl<'v, D: QueryData> ViewQuery<'v, '_, D> {
    /// Calls `f` for every matching entity.
    pub fn for_each(self, mut f: impl FnMut(EntityId, D::Item<'_>)) {
        let (pieces, touched) = self.pieces(usize::MAX);
        for (ids, mut slices) in pieces {
            for (row, &id) in ids.iter().enumerate() {
                f(id, D::get(&mut slices, row));
            }
        }
        touched.record();
    }

    /// Calls `f` for every matching entity in parallel, in fixed `chunk`-row pieces (chunking
    /// depends only on `chunk`, never on the thread count).
    pub fn par_for_each(
        self,
        jobs: &JobSystem,
        chunk: NonZeroUsize,
        f: impl Fn(EntityId, D::Item<'_>) + Sync,
    ) {
        let (mut pieces, touched) = self.pieces(chunk.get());
        jobs.par_chunks_mut(&mut pieces, NonZeroUsize::MIN, |_, work| {
            for (ids, slices) in work.iter_mut() {
                for (row, &id) in ids.iter().enumerate() {
                    f(id, D::get(slices, row));
                }
            }
        });
        drop(pieces);
        touched.record();
    }

    /// The matching archetypes' slices, split into `chunk`-row pieces.
    fn pieces(self, chunk: usize) -> (Vec<Piece<'v, D>>, Touched<'v>) {
        let mutable = self.elems.iter().any(|e| e.mutable);
        let elems = self.elems;
        let View {
            archetypes,
            touched,
            ..
        } = self.view;
        let mut visited = Vec::new();
        let mut pieces = Vec::new();
        for arch in archetypes.iter_mut().filter(|a| !a.ids.is_empty()) {
            let mut picked: Vec<Option<ViewColumn<'_>>> = (0..elems.len()).map(|_| None).collect();
            for (type_id, column) in arch.columns.iter_mut() {
                if let Some(k) = elems.iter().position(|e| e.type_id == *type_id) {
                    picked[k] = Some(ViewColumn(match column {
                        ViewCol::Read(c) => ViewCol::Read(*c),
                        ViewCol::Write(c) => ViewCol::Write(&mut **c),
                    }));
                }
            }
            let Some(columns) = picked.into_iter().collect::<Option<Vec<_>>>() else {
                continue;
            };
            visited.push(arch.index);
            split_chunks::<D>(arch.ids, D::fetch_view(columns), chunk, &mut pieces);
        }
        let record = if mutable { Some(visited) } else { None };
        (pieces, Touched { touched, record })
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
