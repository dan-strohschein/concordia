//! Command buffers (M2 phase 2.0b, ADR-0069): structural changes recorded by processors and
//! applied at the end of their phase. Modeled on Mass's deferred commands (public docs only).

use concordia_core::id::{EntityId, IdSource};

use crate::component::Component;
use crate::error::EcsError;
use crate::world::World;

/// An entity a buffer will spawn; valid only within that buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending(u32);

/// What a command acts on: a live entity, or one the same buffer spawns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// An existing entity.
    Entity(EntityId),
    /// A spawn recorded earlier in the same buffer.
    Pending(Pending),
}

impl From<EntityId> for Target {
    fn from(id: EntityId) -> Self {
        Target::Entity(id)
    }
}

impl From<Pending> for Target {
    fn from(p: Pending) -> Self {
        Target::Pending(p)
    }
}

/// What applying a buffer did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Ids given to the buffer's spawns, in spawn order.
    pub spawned: Vec<EntityId>,
    /// Commands skipped because their target was gone.
    pub skipped: usize,
}

type Edit = Box<dyn FnOnce(&mut World, EntityId) -> Result<(), EcsError> + Send>;

enum Op {
    Spawn,
    Despawn(Target),
    Edit(Target, Edit),
}

/// Recorded structural changes, applied in recording order.
#[derive(Default)]
pub struct CommandBuffer {
    ops: Vec<Op>,
    spawns: u32,
}

impl std::fmt::Debug for CommandBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandBuffer")
            .field("commands", &self.ops.len())
            .field("spawns", &self.spawns)
            .finish()
    }
}

impl CommandBuffer {
    /// An empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of recorded commands.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// True if nothing is recorded.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Records a spawn; the entity gets its id when the buffer is applied.
    pub fn spawn(&mut self) -> Pending {
        self.ops.push(Op::Spawn);
        self.spawns += 1;
        Pending(self.spawns - 1)
    }

    /// Records inserting (or replacing) a component.
    pub fn insert<T: Component>(&mut self, target: impl Into<Target>, value: T) {
        self.edit(target, move |world, id| world.insert(id, value).map(drop));
    }

    /// Records removing a component (nothing happens if the entity lacks it).
    pub fn remove<T: Component>(&mut self, target: impl Into<Target>) {
        self.edit(target, |world, id| world.remove::<T>(id).map(drop));
    }

    /// Records a despawn.
    pub fn despawn(&mut self, target: impl Into<Target>) {
        self.ops.push(Op::Despawn(target.into()));
    }

    /// Records raising signal `S` on an entity (delivered the tick after the buffer applies).
    pub fn signal<S: Component>(&mut self, target: impl Into<Target>, signal: S) {
        self.edit(target, move |world, id| {
            if !world.contains(id) {
                return Err(EcsError::NoSuchEntity(id));
            }
            world.signal(id, signal);
            Ok(())
        });
    }

    /// Appends `other`'s commands after this buffer's.
    pub fn append(&mut self, other: CommandBuffer) {
        let offset = self.spawns;
        let shift = |t: Target| match t {
            Target::Pending(Pending(p)) => Target::Pending(Pending(p + offset)),
            entity => entity,
        };
        self.ops.extend(other.ops.into_iter().map(|op| match op {
            Op::Spawn => Op::Spawn,
            Op::Despawn(t) => Op::Despawn(shift(t)),
            Op::Edit(t, f) => Op::Edit(shift(t), f),
        }));
        self.spawns += other.spawns;
    }

    /// Applies every command in order; spawns take ids from `ids`. Commands whose target is
    /// gone (or whose spawn failed) are skipped and counted.
    pub fn apply(self, world: &mut World, ids: &mut impl IdSource) -> ApplyReport {
        let mut spawned: Vec<Option<EntityId>> = Vec::with_capacity(self.spawns as usize);
        let mut skipped = 0;
        for op in self.ops {
            let ok = match op {
                Op::Spawn => {
                    let id = ids
                        .next_id()
                        .ok()
                        .filter(|id| world.spawn_with_id(*id).is_ok());
                    spawned.push(id);
                    id.is_some()
                }
                Op::Despawn(t) => resolve(&spawned, t).is_some_and(|id| world.despawn(id).is_ok()),
                Op::Edit(t, f) => resolve(&spawned, t).is_some_and(|id| f(world, id).is_ok()),
            };
            skipped += usize::from(!ok);
        }
        ApplyReport {
            spawned: spawned.into_iter().flatten().collect(),
            skipped,
        }
    }

    fn edit(
        &mut self,
        target: impl Into<Target>,
        f: impl FnOnce(&mut World, EntityId) -> Result<(), EcsError> + Send + 'static,
    ) {
        self.ops.push(Op::Edit(target.into(), Box::new(f)));
    }
}

fn resolve(spawned: &[Option<EntityId>], target: Target) -> Option<EntityId> {
    match target {
        Target::Entity(id) => Some(id),
        Target::Pending(Pending(p)) => spawned.get(p as usize).copied().flatten(),
    }
}

/// Runtime counters for a play world (a resource; never hashed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerraStats {
    /// Commands skipped because their target was gone.
    pub skipped_commands: u64,
}
