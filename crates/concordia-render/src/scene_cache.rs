//! Incremental extraction: the engine side keeps each entity's render items and sends only what
//! changed ([`SceneCache`]); the render side keeps the mirror and re-assembles it on change
//! ([`RenderScene`]). Both agree with a full [`RenderWorld::extract`].

use std::collections::{BTreeMap, BTreeSet};

use concordia_core::id::EntityId;
use concordia_core::math::Mat4;
use concordia_terra::World;

use crate::{EntityItems, RenderWorld};

/// Render items by entity (entities with a `Transform`, and environments), in id order.
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
            match crate::extract::placement(world, id) {
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
        for (id, matrix) in crate::extract::placements(world) {
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
/// For a playing world it also keeps the previous tick's items of what moved, to draw between
/// ticks ([`RenderScene::world_at`]).
#[derive(Debug, Default)]
pub struct RenderScene {
    items: SceneItems,
    world: RenderWorld,
    stale: bool,
    previous: SceneItems,
}

impl RenderScene {
    /// Applies the delta of a new simulation tick, remembering the moved entities' previous items.
    pub fn apply_tick(&mut self, delta: SceneDelta) {
        self.previous.clear();
        if let SceneDelta::Changes { upserts, .. } = &delta {
            for (id, _) in upserts {
                if let Some(old) = self.items.get(id) {
                    self.previous.insert(*id, old.clone());
                }
            }
        }
        self.apply(delta);
    }

    /// The scene `alpha` of the way from the previous tick to the latest (clamped to [0, 1]).
    pub fn world_at(&self, alpha: f32) -> RenderWorld {
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        RenderWorld::assemble_owned(self.items.iter().map(
            |(id, now)| match self.previous.get(id) {
                Some(before) => blend(before, now, alpha),
                None => now.clone(),
            },
        ))
    }

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

/// `now`'s items with their transforms `alpha` of the way from `before`'s.
fn blend(before: &EntityItems, now: &EntityItems, alpha: f32) -> EntityItems {
    let mut out = now.clone();
    if let (Some(a), Some(b)) = (&before.draw, &mut out.draw) {
        b.world = lerp_matrix(a.world, b.world, alpha);
    }
    if let (Some(a), Some(b)) = (&before.pose, &mut out.pose) {
        b.world = lerp_matrix(a.world, b.world, alpha);
        if let Ok(pose) = a.pose.lerp(&b.pose, alpha) {
            b.pose = pose;
        }
    }
    if let (Some(a), Some(b)) = (&before.pieces, &mut out.pieces) {
        b.skin.world = lerp_matrix(a.skin.world, b.skin.world, alpha);
        for draw in &mut b.draws {
            draw.world = b.skin.world;
        }
    }
    if let (Some(a), Some(b)) = (&before.camera, &mut out.camera) {
        b.world = lerp_matrix(a.world, b.world, alpha);
    }
    if let (Some(a), Some(b)) = (&before.light, &mut out.light) {
        b.direction = a
            .direction
            .lerp(b.direction, alpha)
            .normalize_or(b.direction);
    }
    for (a, b) in before.punctual.iter().zip(out.punctual.iter_mut()) {
        b.position = a.position.lerp(b.position, alpha);
        b.direction = a
            .direction
            .lerp(b.direction, alpha)
            .normalize_or(b.direction);
    }
    out
}

/// Interpolates scale and translation linearly and rotation spherically.
fn lerp_matrix(a: Mat4, b: Mat4, alpha: f32) -> Mat4 {
    let (sa, ra, ta) = a.to_scale_rotation_translation();
    let (sb, rb, tb) = b.to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(
        sa.lerp(sb, alpha),
        ra.slerp(rb, alpha),
        ta.lerp(tb, alpha),
    )
}
