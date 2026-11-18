//! Systems and schedules. Systems run sequentially in declared order (parallelism is inside systems).

use concordia_core::jobs::JobSystem;
use concordia_core::time::Tick;

use crate::world::World;

/// Context passed to every system run.
pub struct SystemCtx<'a> {
    /// The engine's job system, for parallel queries.
    pub jobs: &'a JobSystem,
    /// The tick being simulated.
    pub tick: Tick,
    /// Fixed step length in seconds.
    pub dt: f32,
}

/// A unit of simulation logic.
pub trait System: Send {
    /// Name (for logs and schedule listings).
    fn name(&self) -> &str;
    /// Runs once per tick.
    fn run(&mut self, world: &mut World, ctx: &SystemCtx<'_>);
}

/// A system made from a closure; see [`system`].
pub struct FnSystem<F> {
    name: String,
    f: F,
}

/// Wraps a closure as a named system.
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

/// Systems in run order.
#[derive(Default)]
pub struct Schedule {
    systems: Vec<Box<dyn System>>,
}

impl Schedule {
    /// An empty schedule.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a system.
    pub fn add(&mut self, system: impl System + 'static) -> &mut Self {
        self.systems.push(Box::new(system));
        self
    }

    /// Appends every system of `other`, keeping its order.
    pub fn append(&mut self, other: Schedule) -> &mut Self {
        self.systems.extend(other.systems);
        self
    }

    /// System names in run order.
    pub fn names(&self) -> Vec<&str> {
        self.systems.iter().map(|s| s.name()).collect()
    }

    /// Runs every system once, in order.
    pub fn run(&mut self, world: &mut World, ctx: &SystemCtx<'_>) {
        for system in &mut self.systems {
            system.run(world, ctx);
        }
    }
}
