//! Incremental extraction: the engine side keeps each entity's render items and sends only what
//! changed ([`SceneCache`]); the render side keeps the mirror and re-assembles it on change
//! ([`RenderScene`]). Both agree with a full [`RenderWorld::extract`].

use std::collections::{BTreeMap, BTreeSet};

use concordia_core::id::EntityId;
use concordia_scene::builtin::{Transform, world_transform};
use concordia_terra::World;

use crate::{EntityItems, RenderWorld};

/// Render items by entity (only entities with a `Transform`), in id order.
pub type SceneItems = BTreeMap<EntityId, EntityItems>;

/// What the render side must apply to catch up with a world.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneDelta {
    /// Everything (a new world, or history too old to diff).
    Full(SceneItems),
    /// New or changed entities, and entities no longer rendered.
    Changes {
        /// Changed entities' new items.
        upserts: Vec<(EntityId, EntityItems)>,
        /// Entities to drop.
        removed: Vec<EntityId>,
    },
}

/// The engine side: the items last sent for one world.
#[derive(Debug, Default)]
pub struct SceneCache {
    world: Option<u64>,
    tick: u64,
    items: SceneItems,
    children: BTreeMap<EntityId, BTreeSet<EntityId>>,
}

impl SceneCache {
    /// Brings the cache up to date with `world` and returns what changed.
    pub fn update(&mut self, world: &World) -> SceneDelta {
        let changes = (self.world == Some(world.identity()))
            .then(|| world.changes_since(self.tick))
            .flatten();
        self.tick = world.change_tick();
        let Some(changes) = changes else {
            return self.rebuild(world);
        };
        let mut dirty: BTreeSet<EntityId> = changes.changed.into_iter().collect();
        for id in &changes.despawned {
            dirty.insert(*id);
        }
        let roots: Vec<EntityId> = dirty.iter().copied().collect();
        for id in roots {
            self.descendants(id, &mut dirty);
        }
        let (mut upserts, mut removed) = (Vec::new(), Vec::new());
        for id in dirty {
            match world_transform(world, id).filter(|_| world.get::<Transform>(id).is_some()) {
                Some(matrix) => {
                    let items = EntityItems::extract(world, id, matrix);
                    self.link(id, items.parent);
                    self.items.insert(id, items.clone());
                    upserts.push((id, items));
                }
                None => {
                    if self.items.contains_key(&id) {
                        self.unlink(id);
                        self.items.remove(&id);
                        removed.push(id);
                    }
                }
            }
        }
        SceneDelta::Changes { upserts, removed }
    }

    fn rebuild(&mut self, world: &World) -> SceneDelta {
        self.world = Some(world.identity());
        self.items.clear();
        self.children.clear();
        for (&id, &matrix) in concordia_scene::builtin::world_transforms(world).iter() {
            let items = EntityItems::extract(world, id, matrix);
            self.link(id, items.parent);
            self.items.insert(id, items);
        }
        SceneDelta::Full(self.items.clone())
    }

    /// Adds every (known) descendant of `id` to `out`.
    fn descendants(&self, id: EntityId, out: &mut BTreeSet<EntityId>) {
        let mut stack = vec![id];
        let mut seen = BTreeSet::from([id]);
        while let Some(next) = stack.pop() {
            for &child in self.children.get(&next).into_iter().flatten() {
                if seen.insert(child) {
                    out.insert(child);
                    stack.push(child);
                }
            }
        }
    }

    fn link(&mut self, id: EntityId, parent: Option<EntityId>) {
        self.unlink(id);
        if let Some(parent) = parent {
            self.children.entry(parent).or_default().insert(id);
        }
    }

    fn unlink(&mut self, id: EntityId) {
        if let Some(old) = self.items.get(&id).and_then(|i| i.parent)
            && let Some(siblings) = self.children.get_mut(&old)
        {
            siblings.remove(&id);
        }
    }
}

/// The render side: the mirror of a [`SceneCache`], assembled into a [`RenderWorld`] on demand.
#[derive(Debug, Default)]
pub struct RenderScene {
    items: SceneItems,
    world: RenderWorld,
    stale: bool,
}

impl RenderScene {
    /// Applies a delta from the engine side.
    pub fn apply(&mut self, delta: SceneDelta) {
        match delta {
            SceneDelta::Full(items) => self.items = items,
            SceneDelta::Changes { upserts, removed } => {
                if upserts.is_empty() && removed.is_empty() {
                    return;
                }
                for id in removed {
                    self.items.remove(&id);
                }
                self.items.extend(upserts);
            }
        }
        self.stale = true;
    }

    /// The current render world.
    pub fn world(&mut self) -> &RenderWorld {
        if self.stale {
            self.world = RenderWorld::assemble(self.items.values());
            self.stale = false;
        }
        &self.world
    }
}
