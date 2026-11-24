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

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::time::Tick;

use crate::command::{CommandBuffer, TerraStats};
use crate::error::ScheduleError;
use crate::lod::Due;
use crate::observer::{Observe, TypeKey};
use crate::processor::{Access, Phase, Processor, ProcessorDecl};
use crate::query::ViewCol;
use crate::resource::Resource;
use crate::view::{View, ViewArch, ViewRes};
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
    /// (`after`, `before`) names in the same phase, honored only when scheduled (another
    /// module's nodes may be absent).
    fn order_if_present(&self) -> (&[String], &[String]) {
        (&[], &[])
    }
}

/// A system made from a closure; see [`system`].
pub struct FnSystem<F> {
    name: String,
    f: F,
    after: Vec<String>,
    before: Vec<String>,
}

impl<F> FnSystem<F> {
    /// Runs after `name` (same phase) when it is scheduled.
    pub fn after_if_present(mut self, name: &str) -> Self {
        self.after.push(name.to_owned());
        self
    }

    /// Runs before `name` (same phase) when it is scheduled.
    pub fn before_if_present(mut self, name: &str) -> Self {
        self.before.push(name.to_owned());
        self
    }
}

/// Wraps a closure as a named exclusive system.
pub fn system<F>(name: impl Into<String>, f: F) -> FnSystem<F>
where
    F: FnMut(&mut World, &SystemCtx<'_>) + Send,
{
    FnSystem {
        name: name.into(),
        f,
        after: Vec::new(),
        before: Vec::new(),
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

    fn order_if_present(&self) -> (&[String], &[String]) {
        (&self.after, &self.before)
    }
}

/// Read-only logic run after every tick, once the clock has advanced (e.g. replay checkpoints).
pub trait TickEnd: Send {
    /// Name (for listings).
    fn name(&self) -> &str;
    /// Runs once after each tick.
    fn run(&mut self, world: &World);
}

/// An end-of-tick hook made from a closure; see [`tick_end`].
pub struct FnTickEnd<F> {
    name: String,
    f: F,
}

/// Wraps a closure as a named end-of-tick hook.
pub fn tick_end<F>(name: impl Into<String>, f: F) -> FnTickEnd<F>
where
    F: FnMut(&World) + Send,
{
    FnTickEnd {
        name: name.into(),
        f,
    }
}

impl<F> TickEnd for FnTickEnd<F>
where
    F: FnMut(&World) + Send,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, world: &World) {
        (self.f)(world);
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

    fn is_observer(&self) -> bool {
        matches!(self, Node::Processor(p) if p.decl().is_observer())
    }

    /// (`after`, `before`) constraints, and whether their targets must be scheduled.
    fn order(&self) -> (&[String], &[String], bool) {
        match self {
            Node::System { system, .. } => {
                let (after, before) = system.order_if_present();
                (after, before, false)
            }
            Node::Processor(p) => (&p.decl().after, &p.decl().before, true),
        }
    }
}

/// One phase, planned: its waves, and its plan order (the order its command buffers apply in).
struct PhasePlan {
    phase: Phase,
    waves: Vec<Vec<usize>>,
    order: Vec<usize>,
}

/// A checked schedule.
struct Plan {
    phases: Vec<PhasePlan>,
    /// Observers, in registration order.
    observers: Vec<usize>,
}

/// Called with a label and the world after each wave and phase flush of an observed tick.
pub type Observer<'a> = &'a mut dyn FnMut(&str, &World);

/// Processors and exclusive systems, by phase.
#[derive(Default)]
pub struct Schedule {
    nodes: Vec<Node>,
    plan: Option<Plan>,
    tick_ends: Vec<Box<dyn TickEnd>>,
}

/// The phase an exclusive system added with [`Schedule::add`] runs in.
const DEFAULT_PHASE: Phase = Phase::PostPhysics;

/// Most rounds of observers after one phase; later changes wait for the next phase's end.
const OBSERVER_ROUNDS: usize = 8;

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

    /// Appends a processor (or an observer).
    pub fn add_processor(&mut self, processor: impl Processor + 'static) -> &mut Self {
        self.nodes.push(Node::Processor(Box::new(processor)));
        self.plan = None;
        self
    }

    /// Appends everything in `other`, keeping its order.
    pub fn append(&mut self, other: Schedule) -> &mut Self {
        self.nodes.extend(other.nodes);
        self.tick_ends.extend(other.tick_ends);
        self.plan = None;
        self
    }

    /// Appends an end-of-tick hook (it runs after every tick, in registration order).
    pub fn add_tick_end(&mut self, hook: impl TickEnd + 'static) -> &mut Self {
        self.tick_ends.push(Box::new(hook));
        self
    }

    /// End-of-tick hook names in registration order.
    pub fn tick_end_names(&self) -> Vec<&str> {
        self.tick_ends.iter().map(|h| h.name()).collect()
    }

    /// Runs the end-of-tick hooks.
    pub(crate) fn run_tick_end(&mut self, world: &World) {
        for hook in &mut self.tick_ends {
            hook.run(world);
        }
    }

    /// Names in registration order.
    pub fn names(&self) -> Vec<&str> {
        self.nodes.iter().map(Node::name).collect()
    }

    /// The planned waves of each phase, by name (empty until planned); observers are listed
    /// separately by [`Schedule::observers`].
    pub fn waves(&self) -> Vec<(Phase, Vec<Vec<&str>>)> {
        let Some(plan) = &self.plan else {
            return Vec::new();
        };
        plan.phases
            .iter()
            .map(|p| {
                let waves = p
                    .waves
                    .iter()
                    .map(|w| w.iter().map(|&i| self.nodes[i].name()).collect())
                    .collect();
                (p.phase, waves)
            })
            .collect()
    }

    /// Observer names in registration order.
    pub fn observers(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|n| n.is_observer())
            .map(Node::name)
            .collect()
    }

    /// Checks the schedule and orders it; registers its presentation components and observed
    /// types with `world`.
    pub fn plan(&mut self, world: &mut World) -> Result<(), ScheduleError> {
        self.check_names()?;
        self.check_presentation(&world.presentation)?;
        let mut phases = Vec::with_capacity(Phase::ALL.len());
        for phase in Phase::ALL {
            let members: Vec<usize> = (0..self.nodes.len())
                .filter(|&i| self.nodes[i].phase() == phase && !self.nodes[i].is_observer())
                .collect();
            let order = self.constrained_order(&members)?;
            let waves = self.order_waves(&members, &order)?;
            phases.push(PhasePlan {
                phase,
                waves,
                order,
            });
        }
        let observers: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| self.nodes[i].is_observer())
            .collect();
        for node in &self.nodes {
            if let Node::Processor(p) = node {
                let decl = p.decl();
                world
                    .presentation
                    .extend(decl.access.presentation.iter().map(|k| k.name));
                if let Some((key, _)) = &decl.trigger {
                    world.structural.observe([key.name]);
                }
            }
        }
        self.plan = Some(Plan { phases, observers });
        Ok(())
    }

    /// Runs every phase once; plans first if needed (an invalid schedule then runs nothing).
    pub fn run(&mut self, world: &mut World, ctx: &SystemCtx<'_>) {
        self.run_inner(world, ctx, None);
    }

    /// [`Schedule::run`], calling `observe` after each wave (labelled by its systems' names)
    /// and each simulation phase's command flush (`"<Phase> commands"`).
    pub fn run_observed(
        &mut self,
        world: &mut World,
        ctx: &SystemCtx<'_>,
        observe: &mut dyn FnMut(&str, &World),
    ) {
        self.run_inner(world, ctx, Some(observe));
    }

    fn run_inner(
        &mut self,
        world: &mut World,
        ctx: &SystemCtx<'_>,
        mut observe: Option<Observer<'_>>,
    ) {
        // `SimLoop::new` reports planning errors; a direct run of a broken schedule does nothing.
        if self.plan.is_none() && self.plan(world).is_err() {
            return;
        }
        let Some(plan) = self.plan.take() else {
            return;
        };
        for (index, phase) in plan.phases.iter().enumerate() {
            let mut buffers = Vec::new();
            for wave in &phase.waves {
                self.run_wave(world, wave, ctx, &mut buffers);
                if let Some(observe) = observe.as_deref_mut() {
                    let names: Vec<&str> = wave.iter().map(|&i| self.nodes[i].name()).collect();
                    observe(&names.join(", "), world);
                }
            }
            if phase.phase == Phase::Presentation {
                continue;
            }
            let mut ids = DeterministicIdGen::new(ctx.tick.0.wrapping_mul(8) + index as u64);
            buffers.sort_by_key(|(node, _)| phase.order.iter().position(|o| o == node));
            flush(world, buffers.into_iter().map(|(_, b)| b), &mut ids);
            self.run_observers(world, &plan.observers, ctx, &mut ids);
            if let Some(observe) = observe.as_deref_mut()
                && !phase.waves.is_empty()
            {
                observe(&format!("{:?} commands", phase.phase), world);
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

    fn check_presentation(&self, registered: &HashSet<&'static str>) -> Result<(), ScheduleError> {
        let presentation: HashSet<&'static str> = self
            .nodes
            .iter()
            .filter_map(Node::access)
            .flat_map(|a| a.presentation.iter().map(|k| k.name))
            .chain(registered.iter().copied())
            .collect();
        for node in &self.nodes {
            let Some(access) = node.access() else {
                continue;
            };
            let processor = || node.name().to_owned();
            if node.phase() == Phase::Presentation && !node.is_observer() {
                let sim_write = access
                    .writes
                    .iter()
                    .find(|k| !presentation.contains(k.name))
                    .or(access.resource_writes.first());
                if let Some(k) = sim_write {
                    return Err(ScheduleError::PresentationWritesSimulation {
                        processor: processor(),
                        component: k.name.to_owned(),
                    });
                }
            } else if let Some(k) = access
                .reads
                .iter()
                .chain(&access.writes)
                .find(|k| presentation.contains(k.name))
            {
                return Err(ScheduleError::SimulationReadsPresentation {
                    processor: processor(),
                    component: k.name.to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Orders one phase's `members` (already in constrained `order`) into waves of
    /// non-conflicting nodes.
    fn order_waves(
        &self,
        members: &[usize],
        order: &[usize],
    ) -> Result<Vec<Vec<usize>>, ScheduleError> {
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

    /// `(earlier, later)` pairs from before/after constraints within `members`; an exclusive
    /// system's targets that aren't members are skipped, a processor's are an error.
    fn constraint_edges(&self, members: &[usize]) -> Result<Vec<(usize, usize)>, ScheduleError> {
        let find = |from: usize, name: &str, required: bool| {
            let found = members
                .iter()
                .copied()
                .find(|&m| self.nodes[m].name() == name);
            match (found, required) {
                (None, true) => Err(ScheduleError::UnknownProcessor {
                    processor: self.nodes[from].name().to_owned(),
                    target: name.to_owned(),
                }),
                (found, _) => Ok(found),
            }
        };
        let mut edges = Vec::new();
        for &m in members {
            let (after, before, required) = self.nodes[m].order();
            for name in after {
                edges.extend(find(m, name, required)?.map(|t| (t, m)));
            }
            for name in before {
                edges.extend(find(m, name, required)?.map(|t| (m, t)));
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
    /// Processors' command buffers go to `buffers`, tagged with their node index.
    fn run_wave(
        &mut self,
        world: &mut World,
        wave: &[usize],
        ctx: &SystemCtx<'_>,
        buffers: &mut Vec<(usize, CommandBuffer)>,
    ) {
        if let [single] = wave
            && let Node::System { system, .. } = &mut self.nodes[*single]
        {
            system.run(world, ctx);
            return;
        }
        let (indices, mut members): (Vec<usize>, Vec<&mut Box<dyn Processor>>) = self
            .nodes
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| wave.contains(i))
            .filter_map(|(i, node)| match node {
                Node::Processor(p) => Some((i, p)),
                Node::System { .. } => None,
            })
            .unzip();
        let outputs = run_processors(world, &mut members, &[], ctx);
        let mut touched = Vec::with_capacity(outputs.len());
        for (i, (t, commands)) in indices.into_iter().zip(outputs) {
            touched.push(t);
            buffers.push((i, commands));
        }
        apply_touches(world, touched);
    }

    /// Runs observers on the net adds and removals since the last pass, applying their
    /// commands, for at most [`OBSERVER_ROUNDS`] rounds.
    fn run_observers(
        &mut self,
        world: &mut World,
        observers: &[usize],
        ctx: &SystemCtx<'_>,
        ids: &mut DeterministicIdGen,
    ) {
        if observers.is_empty() {
            return;
        }
        for _ in 0..OBSERVER_ROUNDS {
            let net = world.structural.take_net();
            if net.is_empty() {
                return;
            }
            let mut buffers = Vec::new();
            for &i in observers {
                let Node::Processor(p) = &mut self.nodes[i] else {
                    continue;
                };
                let Some((key, on)) = p.decl().trigger else {
                    continue;
                };
                let Some(observed) = net.get(&(TypeKey(key.name), on == Observe::Add)) else {
                    continue;
                };
                let mut one = [p];
                let outputs = run_processors(world, &mut one, observed, ctx);
                for (touched, commands) in outputs {
                    apply_touches(world, vec![touched]);
                    buffers.push(commands);
                }
            }
            flush(world, buffers.into_iter(), ids);
        }
    }
}

/// Applies `buffers` in order; skipped commands are counted in [`TerraStats`].
fn flush(
    world: &mut World,
    buffers: impl Iterator<Item = CommandBuffer>,
    ids: &mut DeterministicIdGen,
) {
    let mut skipped = 0u64;
    for buffer in buffers.filter(|b| !b.is_empty()) {
        skipped += buffer.apply(world, ids).skipped as u64;
    }
    if skipped > 0 {
        if world.resource::<TerraStats>().is_none() {
            world.insert_resource(TerraStats::default());
        }
        if let Some(stats) = world.resource_mut::<TerraStats>() {
            stats.skipped_commands += skipped;
        }
    }
}

/// What one processor run left behind: the archetypes its mutable queries visited, and its
/// command buffer.
type Output = (Vec<Vec<usize>>, CommandBuffer);

/// Runs non-conflicting processors in parallel (`observed` goes to every view, for observers).
fn run_processors(
    world: &mut World,
    members: &mut [&mut Box<dyn Processor>],
    observed: &[EntityId],
    ctx: &SystemCtx<'_>,
) -> Vec<Output> {
    let decls: Vec<ProcessorDecl> = members.iter().map(|p| p.decl().clone()).collect();
    let mut lent = take_resources(world, &decls);
    let outputs = {
        let mut views = build_views(world, &decls, &mut lent);
        for view in &mut views {
            view.observed = observed.to_vec();
            view.step = Due {
                dt: ctx.dt,
                dt_exact: ctx.dt_exact,
                level: 0,
            };
        }
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
        views.into_iter().map(|v| (v.touched, v.commands)).collect()
    };
    for (t, value) in lent {
        world.resources.put(t, value);
    }
    outputs
}

/// Takes every resource the processors declared out of the world, to lend to their views.
fn take_resources(world: &mut World, decls: &[ProcessorDecl]) -> Vec<(TypeId, Box<dyn Resource>)> {
    let mut lent: Vec<(TypeId, Box<dyn Resource>)> = Vec::new();
    for decl in decls {
        for (key, _) in decl.access.resources() {
            if lent.iter().all(|(t, _)| *t != key.type_id)
                && let Some(value) = world.resources.take(key.type_id)
            {
                lent.push((key.type_id, value));
            }
        }
    }
    lent
}

/// One view per processor, holding only its declared columns, resources and signals.
fn build_views<'w>(
    world: &'w mut World,
    decls: &'w [ProcessorDecl],
    lent: &'w mut [(TypeId, Box<dyn Resource>)],
) -> Vec<View<'w>> {
    let World {
        archetypes,
        components,
        entities,
        signals,
        shared,
        ..
    } = world;
    let mut views: Vec<View<'w>> = decls
        .iter()
        .map(|decl| View {
            name: &decl.name,
            phase: decl.phase,
            access: &decl.access,
            archetypes: Vec::new(),
            entities: &*entities,
            signals: &*signals,
            resources: Vec::new(),
            observed: Vec::new(),
            step: Due {
                dt: 0.0,
                dt_exact: 0.0,
                level: 0,
            },
            touched: Vec::new(),
            commands: CommandBuffer::new(),
        })
        .collect();
    lend_resources(&mut views, decls, lent);
    // Per view: declared name → component id (unseen types have no columns).
    let declared: Vec<Vec<(&'static str, crate::component::ComponentId, bool)>> = decls
        .iter()
        .map(|d| {
            let a = &d.access;
            let writes: HashSet<&'static str> = a.written().map(|k| k.name).collect();
            a.touched()
                .filter_map(|k| {
                    components
                        .by_name(k.name)
                        .map(|c| (components.info(c).name, c, writes.contains(k.name)))
                })
                .collect()
        })
        .collect();
    for (index, arch) in archetypes.list.iter_mut().enumerate() {
        let comps = &arch.components;
        let table = &mut arch.table;
        let ids: &'w [_] = &table.ids;
        let mut per_view: Vec<Vec<(&'static str, ViewCol<'w>)>> =
            (0..views.len()).map(|_| Vec::new()).collect();
        for (column, &comp) in table.columns.iter_mut().zip(comps.iter()) {
            let users: Vec<(usize, &'static str, bool)> = declared
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
        for &(comp, value) in &arch.shared {
            let one: &'w dyn concordia_reflect::DynReflect = shared.get_reflect(comp, value);
            for (v, d) in declared.iter().enumerate() {
                if let Some((t, _, _)) = d.iter().find(|(_, c, _)| *c == comp) {
                    per_view[v].push((*t, ViewCol::Shared(one)));
                }
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

/// Gives each view its declared resources: exclusive to the one writer, shared among readers.
fn lend_resources<'w>(
    views: &mut [View<'w>],
    decls: &[ProcessorDecl],
    lent: &'w mut [(TypeId, Box<dyn Resource>)],
) {
    for (t, value) in lent.iter_mut() {
        let users: Vec<(usize, bool)> = decls
            .iter()
            .enumerate()
            .filter_map(|(v, d)| {
                d.access
                    .resources()
                    .find(|(k, _)| k.type_id == *t)
                    .map(|(_, w)| (v, w))
            })
            .collect();
        match users.iter().find(|(_, write)| *write) {
            Some(&(v, _)) => {
                debug_assert!(users.len() == 1, "a wave never shares a written resource");
                views[v].resources.push((*t, ViewRes::Write(&mut **value)));
            }
            None => {
                let shared: &'w dyn Resource = &**value;
                for &(v, _) in &users {
                    views[v].resources.push((*t, ViewRes::Read(shared)));
                }
            }
        }
    }
}

/// Marks archetypes visited by mutable queries as changed (one change tick per query).
fn apply_touches(world: &mut World, touched: Vec<Vec<Vec<usize>>>) {
    for visited in touched.into_iter().flatten() {
        let tick = world.changes.bump();
        for arch in visited {
            world.archetypes.list[arch].changed = tick;
        }
    }
}
