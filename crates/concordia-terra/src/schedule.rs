//! Schedules: processors and exclusive systems by processing phase (ADR-0063).
//!
//! Each phase runs in waves. A wave is a set of processors that don't conflict (none writes what
//! another touches); they run in parallel, each with a view of only its declared columns.
//! Exclusive systems (full `&mut World`) form a wave on their own. The order is deterministic:
//! explicit before/after constraints first, then registration order, so a parallel run gives the
//! same result as running everything one by one.

use std::any::TypeId;
use std::collections::HashSet;
use std::num::NonZeroUsize;

use concordia_core::jobs::JobSystem;
use concordia_core::time::Tick;

use crate::error::ScheduleError;
use crate::processor::{Access, Phase, Processor, View, ViewArch};
use crate::query::ViewCol;
use crate::world::World;

/// Context passed to every system and processor run.
pub struct SystemCtx<'a> {
    /// The engine's job system, for parallel queries.
    pub jobs: &'a JobSystem,
    /// The tick being simulated.
    pub tick: Tick,
    /// This tick's simulated seconds: the fixed step times the time scale.
    pub dt: f32,
    /// The same, at full precision (for timers and clocks).
    pub dt_exact: f64,
}

/// A unit of simulation logic that sees the whole world (an exclusive system).
pub trait System: Send {
    /// Name (for logs, schedule listings and order constraints).
    fn name(&self) -> &str;
    /// Runs once per tick.
    fn run(&mut self, world: &mut World, ctx: &SystemCtx<'_>);
}

/// A system made from a closure; see [`system`].
pub struct FnSystem<F> {
    name: String,
    f: F,
}

/// Wraps a closure as a named exclusive system.
pub fn system<F>(name: impl Into<String>, f: F) -> FnSystem<F>
where
    F: FnMut(&mut World, &SystemCtx<'_>) + Send,
{
    FnSystem {
        name: name.into(),
        f,
    }
}

impl<F> System for FnSystem<F>
where
    F: FnMut(&mut World, &SystemCtx<'_>) + Send,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, world: &mut World, ctx: &SystemCtx<'_>) {
        (self.f)(world, ctx);
    }
}

/// A scheduled unit: an exclusive system or a processor.
enum Node {
    System {
        phase: Phase,
        system: Box<dyn System>,
    },
    Processor(Box<dyn Processor>),
}

impl Node {
    fn name(&self) -> &str {
        match self {
            Node::System { system, .. } => system.name(),
            Node::Processor(p) => p.decl().name(),
        }
    }

    fn phase(&self) -> Phase {
        match self {
            Node::System { phase, .. } => *phase,
            Node::Processor(p) => p.decl().phase(),
        }
    }

    /// `None` for an exclusive system (it conflicts with everything).
    fn access(&self) -> Option<&Access> {
        match self {
            Node::System { .. } => None,
            Node::Processor(p) => Some(&p.decl().access),
        }
    }

    fn conflicts(&self, other: &Node) -> bool {
        match (self.access(), other.access()) {
            (Some(a), Some(b)) => a.conflicts(b),
            _ => true,
        }
    }

    /// (`after`, `before`) constraints.
    fn order(&self) -> (&[String], &[String]) {
        match self {
            Node::System { .. } => (&[], &[]),
            Node::Processor(p) => (&p.decl().after, &p.decl().before),
        }
    }
}

/// Processors and exclusive systems, by phase.
#[derive(Default)]
pub struct Schedule {
    nodes: Vec<Node>,
    /// Per phase (in `Phase::ALL` order), waves of node indices; built by [`Schedule::plan`].
    plan: Option<Vec<Vec<Vec<usize>>>>,
}

/// The phase an exclusive system added with [`Schedule::add`] runs in.
const DEFAULT_PHASE: Phase = Phase::PostPhysics;

impl Schedule {
    /// An empty schedule.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an exclusive system in the `PostPhysics` phase.
    pub fn add(&mut self, system: impl System + 'static) -> &mut Self {
        self.add_in(DEFAULT_PHASE, system)
    }

    /// Appends an exclusive system (it sees the whole world and runs alone) in `phase`.
    pub fn add_in(&mut self, phase: Phase, system: impl System + 'static) -> &mut Self {
        self.nodes.push(Node::System {
            phase,
            system: Box::new(system),
        });
        self.plan = None;
        self
    }

    /// Appends a processor.
    pub fn add_processor(&mut self, processor: impl Processor + 'static) -> &mut Self {
        self.nodes.push(Node::Processor(Box::new(processor)));
        self.plan = None;
        self
    }

    /// Appends everything in `other`, keeping its order.
    pub fn append(&mut self, other: Schedule) -> &mut Self {
        self.nodes.extend(other.nodes);
        self.plan = None;
        self
    }

    /// Names in registration order.
    pub fn names(&self) -> Vec<&str> {
        self.nodes.iter().map(Node::name).collect()
    }

    /// Checks the schedule and orders it; registers its presentation components with `world`.
    pub fn plan(&mut self, world: &mut World) -> Result<(), ScheduleError> {
        self.check_names()?;
        self.check_presentation(&world.presentation)?;
        let mut phases = Vec::with_capacity(Phase::ALL.len());
        for phase in Phase::ALL {
            let members: Vec<usize> = (0..self.nodes.len())
                .filter(|&i| self.nodes[i].phase() == phase)
                .collect();
            phases.push(self.waves(&members)?);
        }
        for node in &self.nodes {
            if let Some(access) = node.access() {
                world
                    .presentation
                    .extend(access.presentation.iter().map(|k| k.type_id));
            }
        }
        self.plan = Some(phases);
        Ok(())
    }

    /// Runs every phase once; plans first if needed (an invalid schedule then runs nothing).
    pub fn run(&mut self, world: &mut World, ctx: &SystemCtx<'_>) {
        // `SimLoop::new` reports planning errors; a direct run of a broken schedule does nothing.
        if self.plan.is_none() && self.plan(world).is_err() {
            return;
        }
        let Some(plan) = self.plan.take() else {
            return;
        };
        for waves in &plan {
            for wave in waves {
                self.run_wave(world, wave, ctx);
            }
        }
        self.plan = Some(plan);
    }

    fn check_names(&self) -> Result<(), ScheduleError> {
        let mut seen = HashSet::new();
        for node in &self.nodes {
            if !seen.insert(node.name()) {
                return Err(ScheduleError::DuplicateName {
                    name: node.name().to_owned(),
                });
            }
        }
        Ok(())
    }

    fn check_presentation(&self, registered: &HashSet<TypeId>) -> Result<(), ScheduleError> {
        let presentation: HashSet<TypeId> = self
            .nodes
            .iter()
            .filter_map(Node::access)
            .flat_map(|a| a.presentation.iter().map(|k| k.type_id))
            .chain(registered.iter().copied())
            .collect();
        for node in &self.nodes {
            let Some(access) = node.access() else {
                continue;
            };
            let processor = || node.name().to_owned();
            if node.phase() == Phase::Presentation {
                if let Some(k) = access
                    .writes
                    .iter()
                    .find(|k| !presentation.contains(&k.type_id))
                {
                    return Err(ScheduleError::PresentationWritesSimulation {
                        processor: processor(),
                        component: k.name.to_owned(),
                    });
                }
            } else if let Some(k) = access
                .reads
                .iter()
                .chain(&access.writes)
                .find(|k| presentation.contains(&k.type_id))
            {
                return Err(ScheduleError::SimulationReadsPresentation {
                    processor: processor(),
                    component: k.name.to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Orders one phase's `members` into waves of non-conflicting nodes.
    fn waves(&self, members: &[usize]) -> Result<Vec<Vec<usize>>, ScheduleError> {
        let order = self.constrained_order(members)?;
        // Edges: explicit constraints, plus every conflicting pair in `order`.
        let n = order.len();
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        let pos = |node: usize| order.iter().position(|&o| o == node);
        for (a, b) in self.constraint_edges(members)? {
            if let (Some(pa), Some(pb)) = (pos(a), pos(b)) {
                preds[pb].push(pa);
            }
        }
        for j in 0..n {
            for i in 0..j {
                if self.nodes[order[i]].conflicts(&self.nodes[order[j]]) {
                    preds[j].push(i);
                }
            }
        }
        // Each node's wave is one past its latest predecessor's.
        let mut level = vec![0usize; n];
        for j in 0..n {
            level[j] = preds[j].iter().map(|&p| level[p] + 1).max().unwrap_or(0);
        }
        let depth = level.iter().max().map_or(0, |d| d + 1);
        let mut waves = vec![Vec::new(); depth];
        for (j, &l) in level.iter().enumerate() {
            waves[l].push(order[j]);
        }
        Ok(waves)
    }

    /// `(earlier, later)` pairs from before/after constraints within `members`.
    fn constraint_edges(&self, members: &[usize]) -> Result<Vec<(usize, usize)>, ScheduleError> {
        let find = |from: usize, name: &str| {
            members
                .iter()
                .copied()
                .find(|&m| self.nodes[m].name() == name)
                .ok_or_else(|| ScheduleError::UnknownProcessor {
                    processor: self.nodes[from].name().to_owned(),
                    target: name.to_owned(),
                })
        };
        let mut edges = Vec::new();
        for &m in members {
            let (after, before) = self.nodes[m].order();
            for name in after {
                edges.push((find(m, name)?, m));
            }
            for name in before {
                edges.push((m, find(m, name)?));
            }
        }
        Ok(edges)
    }

    /// A topological order of `members` under the constraints, ties broken by registration order.
    fn constrained_order(&self, members: &[usize]) -> Result<Vec<usize>, ScheduleError> {
        let edges = self.constraint_edges(members)?;
        let mut indegree: Vec<usize> = members
            .iter()
            .map(|&m| edges.iter().filter(|(_, b)| *b == m).count())
            .collect();
        let mut done = vec![false; members.len()];
        let mut order = Vec::with_capacity(members.len());
        while order.len() < members.len() {
            let Some(k) = (0..members.len()).find(|&k| !done[k] && indegree[k] == 0) else {
                let processors = (0..members.len())
                    .filter(|&k| !done[k])
                    .map(|k| self.nodes[members[k]].name().to_owned())
                    .collect();
                return Err(ScheduleError::Cycle { processors });
            };
            done[k] = true;
            order.push(members[k]);
            for &(_, b) in edges.iter().filter(|(a, _)| *a == members[k]) {
                if let Some(kb) = members.iter().position(|&m| m == b) {
                    indegree[kb] -= 1;
                }
            }
        }
        Ok(order)
    }

    /// Runs one wave: an exclusive system alone, or processors in parallel on disjoint views.
    fn run_wave(&mut self, world: &mut World, wave: &[usize], ctx: &SystemCtx<'_>) {
        if let [single] = wave
            && let Node::System { system, .. } = &mut self.nodes[*single]
        {
            system.run(world, ctx);
            return;
        }
        let mut members: Vec<&mut Box<dyn Processor>> = self
            .nodes
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| wave.contains(i))
            .filter_map(|(_, node)| match node {
                Node::Processor(p) => Some(p),
                Node::System { .. } => None,
            })
            .collect();
        let touched = run_processors(world, &mut members, ctx);
        apply_touches(world, touched);
    }
}

/// Runs non-conflicting processors in parallel; returns, per processor in order, the archetypes
/// its mutable queries visited.
fn run_processors(
    world: &mut World,
    members: &mut [&mut Box<dyn Processor>],
    ctx: &SystemCtx<'_>,
) -> Vec<Vec<Vec<usize>>> {
    let accesses: Vec<Access> = members.iter().map(|p| p.decl().access.clone()).collect();
    let mut views = build_views(world, &accesses);
    let mut work: Vec<(&mut Box<dyn Processor>, &mut View<'_>)> = members
        .iter_mut()
        .map(|p| &mut **p)
        .zip(views.iter_mut())
        .collect();
    ctx.jobs
        .par_chunks_mut(&mut work, NonZeroUsize::MIN, |_, items| {
            for (processor, view) in items.iter_mut() {
                processor.run(view, ctx);
            }
        });
    drop(work);
    views.into_iter().map(|v| v.touched).collect()
}

/// One view per access, each holding only its declared columns of every archetype.
fn build_views<'w>(world: &'w mut World, accesses: &'w [Access]) -> Vec<View<'w>> {
    let World {
        archetypes,
        components,
        ..
    } = world;
    let mut views: Vec<View<'w>> = accesses
        .iter()
        .map(|access| View {
            access,
            archetypes: Vec::new(),
            touched: Vec::new(),
        })
        .collect();
    // Per view: declared type → component id (unseen types have no columns).
    let declared: Vec<Vec<(TypeId, crate::component::ComponentId, bool)>> = accesses
        .iter()
        .map(|a| {
            let writes: HashSet<TypeId> = a.written().map(|k| k.type_id).collect();
            a.touched()
                .filter_map(|k| {
                    components
                        .id_of(k.type_id)
                        .map(|c| (k.type_id, c, writes.contains(&k.type_id)))
                })
                .collect()
        })
        .collect();
    for (index, arch) in archetypes.list.iter_mut().enumerate() {
        let comps = &arch.components;
        let table = &mut arch.table;
        let ids: &'w [_] = &table.ids;
        let mut per_view: Vec<Vec<(TypeId, ViewCol<'w>)>> =
            (0..views.len()).map(|_| Vec::new()).collect();
        for (column, &comp) in table.columns.iter_mut().zip(comps.iter()) {
            let users: Vec<(usize, TypeId, bool)> = declared
                .iter()
                .enumerate()
                .filter_map(|(v, d)| {
                    d.iter()
                        .find(|(_, c, _)| *c == comp)
                        .map(|(t, _, w)| (v, *t, *w))
                })
                .collect();
            match users.iter().find(|(_, _, write)| *write) {
                Some(&(v, t, _)) => {
                    debug_assert!(users.len() == 1, "a wave never shares a written column");
                    per_view[v].push((t, ViewCol::Write(&mut **column)));
                }
                None if !users.is_empty() => {
                    let shared: &'w dyn crate::storage::column::Column = &**column;
                    for &(v, t, _) in &users {
                        per_view[v].push((t, ViewCol::Read(shared)));
                    }
                }
                None => {}
            }
        }
        for (view, columns) in views.iter_mut().zip(per_view) {
            if !columns.is_empty() {
                view.archetypes.push(ViewArch {
                    index,
                    ids,
                    columns,
                });
            }
        }
    }
    views
}

/// Marks entities of archetypes visited by mutable queries as changed (one change tick each).
fn apply_touches(world: &mut World, touched: Vec<Vec<Vec<usize>>>) {
    for visited in touched.into_iter().flatten() {
        let tick = world.changes.bump();
        for arch in visited {
            for &id in &world.archetypes.list[arch].table.ids {
                world.entities.touch(id, tick);
            }
        }
    }
}
