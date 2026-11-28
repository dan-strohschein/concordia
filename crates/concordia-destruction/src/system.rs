//! The destruction system (play only, after physics): set breakables up, damage bonds from
//! impacts, break at most a budget of bonds, and split fragments into their connected parts.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use concordia_core::id::{EntityId, TypedId};
use concordia_physics::{
    BodyKind, Collider, ColliderShape, ContactImpact, ContactReports, Impulse, PhysicsDisabled,
    RigidBody, Sleep,
};
use concordia_scene::builtin::{Transform, Velocity};
use concordia_terra::{SystemCtx, Without, World, system};

use crate::prepared::Prepared;
use concordia_core::math::Vec3;

use crate::{
    Breakable, BreakableSource, DebrisAge, Field, FieldAge, FieldEffect, FieldShape, Fragment,
};

/// Runtime limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DestructionSettings {
    /// The most bonds that break in one tick; the rest wait for later ticks.
    pub breaks_per_tick: u32,
    /// The most debris (parts split off) alive at once; past it, the oldest go first.
    pub max_debris: u32,
    /// Seconds debris lives (0: for ever).
    pub debris_lifetime: f32,
    /// Debris smaller than this (m³) loses its collision.
    pub min_debris_volume: f32,
}

impl Default for DestructionSettings {
    fn default() -> Self {
        Self {
            breaks_per_tick: 64,
            max_debris: 512,
            debris_lifetime: 0.0,
            min_debris_volume: 0.0,
        }
    }
}

/// The system `destruction.break`, for the `PostPhysics` phase of a play schedule.
pub fn destruction_system(
    source: Arc<dyn BreakableSource>,
    settings: DestructionSettings,
) -> impl concordia_terra::System {
    let mut state = State {
        source,
        settings,
        prepared: BTreeMap::new(),
        failed: BTreeSet::new(),
    };
    system(
        "destruction.break",
        move |world: &mut World, ctx: &SystemCtx<'_>| state.tick(world, ctx.dt),
    )
}

struct State {
    source: Arc<dyn BreakableSource>,
    settings: DestructionSettings,
    prepared: BTreeMap<String, Arc<Prepared>>,
    failed: BTreeSet<String>,
}

impl State {
    fn tick(&mut self, world: &mut World, dt: f32) {
        self.set_up(world);
        self.fields(world, dt);
        self.damage(world);
        self.break_bonds(world);
        self.debris(world, dt);
    }

    /// Ages debris, removes what is past its lifetime or over budget, drops small debris' collision.
    fn debris(&mut self, world: &mut World, dt: f32) {
        let mut debris = Vec::new();
        if let Ok(mut q) = world.query_filtered::<&Fragment, Without<Breakable>>() {
            q.for_each(world, |id, f| debris.push((id, f.clone())));
        }
        debris.sort_by_key(|(id, _)| *id);
        let mut alive: Vec<(f32, EntityId)> = Vec::new();
        for (id, f) in debris {
            let age = world.get::<DebrisAge>(id).map_or(0.0, |a| a.seconds + dt);
            let s = self.settings;
            if s.debris_lifetime > 0.0 && age >= s.debris_lifetime {
                let _ = world.despawn(id);
                continue;
            }
            let _ = world.insert(id, DebrisAge { seconds: age });
            if s.min_debris_volume > 0.0
                && let Some(p) = self.prepared(&f.recipe)
            {
                let volume: f64 = f
                    .pieces
                    .iter()
                    .map(|&q| p.loaded.fracture.pieces[q as usize].volume)
                    .sum();
                if volume < f64::from(s.min_debris_volume) {
                    let _ = world.remove::<Collider>(id);
                }
            }
            alive.push((age, id));
        }
        // Over budget: the oldest go first (ties: lower id first).
        let excess = alive
            .len()
            .saturating_sub(self.settings.max_debris as usize);
        alive.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, id) in alive.into_iter().take(excess) {
            let _ = world.despawn(id);
        }
    }

    fn prepared(&mut self, recipe: &str) -> Option<Arc<Prepared>> {
        if let Some(p) = self.prepared.get(recipe) {
            return Some(p.clone());
        }
        if self.failed.contains(recipe) {
            return None;
        }
        match self.source.load(recipe) {
            Ok(loaded) => {
                let p = Arc::new(Prepared::new(loaded));
                self.prepared.insert(recipe.to_owned(), p.clone());
                Some(p)
            }
            Err(e) => {
                tracing::warn!(target: "concordia::destruction", error = %e, "breakable skipped");
                self.failed.insert(recipe.to_owned());
                None
            }
        }
    }

    /// Breakables without a fragment become one: a compound of all their pieces.
    fn set_up(&mut self, world: &mut World) {
        let mut new = Vec::new();
        if let Ok(mut q) = world.query::<&Breakable>() {
            q.for_each(world, |id, b| new.push((id, b.clone())));
        }
        new.sort_by_key(|(id, _)| *id);
        for (id, b) in new {
            if world.get::<Fragment>(id).is_some() {
                continue;
            }
            let Some(p) = self.prepared(&b.recipe) else {
                continue;
            };
            let pieces: Vec<u32> = (0..p.loaded.fracture.pieces.len() as u32).collect();
            let fragment = Fragment {
                recipe: b.recipe.clone(),
                anchors: b.anchors.clone(),
                impact_radius: b.impact_radius,
                damage: vec![0.0; p.bond_pieces.len()],
                broken: Vec::new(),
                pieces,
            };
            let body = world
                .get::<RigidBody>(id)
                .cloned()
                .unwrap_or_else(RigidBody::dynamic);
            let count = fragment.pieces.len() as u32;
            let drawn = crate::looks::draw_whole(world, id, p.loaded.looks.as_ref(), count);
            if let Err(e) = drawn.and_then(|()| attach(world, id, &p, fragment, body)) {
                tracing::warn!(target: "concordia::destruction", %id, error = %e, "breakable skipped");
            }
        }
    }

    /// Fields act on fragments, before impacts and in id order, at the same point every tick.
    fn fields(&mut self, world: &mut World, dt: f32) {
        let fields = active_fields(world, dt);
        if fields.is_empty() {
            return;
        }
        let mut fragments = Vec::new();
        if let Ok(mut q) = world.query::<(&Fragment, &Transform)>() {
            q.for_each(world, |id, (f, t)| fragments.push((id, f.clone(), *t)));
        }
        fragments.sort_by_key(|(id, ..)| *id);
        for (id, fragment, t) in fragments {
            let Some(p) = self.prepared(&fragment.recipe) else {
                continue;
            };
            let debris = world.get::<Breakable>(id).is_none();
            let Some(next) = apply_fields(&p, &fields, fragment.clone(), &t, debris, dt) else {
                let _ = world.despawn(id);
                continue;
            };
            let pushes = pushes(&p, &fields, &next, &t, dt);
            if next != fragment {
                let pinned = !anchored(&p, &fragment) && anchored(&p, &next);
                let _ = world.insert(id, next);
                if pinned {
                    pin(world, id);
                }
            }
            pushes.apply(world, id);
        }
    }

    /// Each impact on a fragment damages its unbroken bonds within `impact_radius`.
    fn damage(&mut self, world: &mut World) {
        let impacts: Vec<(EntityId, ContactImpact)> = world.signals::<ContactImpact>().to_vec();
        for (id, impact) in impacts {
            let (Some(fragment), Some(t)) = (
                world.get::<Fragment>(id).cloned(),
                world.get::<Transform>(id).copied(),
            ) else {
                continue;
            };
            let Some(p) = self.prepared(&fragment.recipe) else {
                continue;
            };
            let local = t.rotation.inverse() * (impact.point - t.translation);
            let radius = fragment.impact_radius.max(f32::EPSILON);
            let mut next = fragment.clone();
            for k in p.internal_bonds(&fragment.pieces) {
                if fragment.broken.binary_search(&(k as u32)).is_ok() {
                    continue;
                }
                let falloff = 1.0 - (p.bond_centre[k] - local).length() / radius;
                if falloff > 0.0 {
                    next.damage[k] += impact.strength * falloff;
                }
            }
            if next != fragment {
                let _ = world.insert(id, next);
            }
        }
    }

    /// Breaks bonds past their strength (at most the budget, by fragment then bond) and splits.
    fn break_bonds(&mut self, world: &mut World) {
        let mut fragments = Vec::new();
        if let Ok(mut q) = world.query::<&Fragment>() {
            q.for_each(world, |id, f| fragments.push((id, f.clone())));
        }
        fragments.sort_by_key(|(id, _)| *id);
        let mut budget = self.settings.breaks_per_tick as usize;
        for (id, mut fragment) in fragments {
            if budget == 0 {
                break;
            }
            let Some(p) = self.prepared(&fragment.recipe) else {
                continue;
            };
            let due: Vec<u32> = p
                .internal_bonds(&fragment.pieces)
                .filter(|&k| {
                    fragment.broken.binary_search(&(k as u32)).is_err()
                        && f64::from(fragment.damage[k]) >= p.loaded.structure.edges[k].strength
                })
                .map(|k| k as u32)
                .take(budget)
                .collect();
            if due.is_empty() {
                continue;
            }
            budget -= due.len();
            fragment.broken.extend(due);
            fragment.broken.sort_unstable();
            if let Err(e) = split(world, id, &p, fragment) {
                tracing::warn!(target: "concordia::destruction", %id, error = %e, "split failed");
            }
        }
    }
}

/// A field acting this tick: where it is, and whether this is its first tick.
type Acting = (Field, Transform, bool);

/// Fields that act this tick, in id order; expired ones leave the play world.
fn active_fields(world: &mut World, dt: f32) -> Vec<Acting> {
    let mut all = Vec::new();
    if let Ok(mut q) = world.query::<(&Field, &Transform)>() {
        q.for_each(world, |id, (f, t)| all.push((id, f.clone(), *t)));
    }
    all.sort_by_key(|(id, ..)| *id);
    let mut active = Vec::new();
    for (id, f, t) in all {
        let age = world.get::<FieldAge>(id).copied();
        let seconds = age.map_or(0.0, |a| a.seconds);
        if f.duration > 0.0 && seconds >= f.duration {
            let _ = world.despawn(id);
            continue;
        }
        let _ = world.insert(
            id,
            FieldAge {
                seconds: seconds + dt,
            },
        );
        active.push((f, t, age.is_none()));
    }
    active
}

/// One fragment under every field: its new state, or `None` if a kill field removes it.
fn apply_fields(
    p: &Prepared,
    fields: &[Acting],
    mut f: Fragment,
    t: &Transform,
    debris: bool,
    dt: f32,
) -> Option<Fragment> {
    let world_point = |local: Vec3| t.translation + t.rotation * local;
    let bonds: Vec<usize> = p
        .internal_bonds(&f.pieces)
        .filter(|k| f.broken.binary_search(&(*k as u32)).is_err())
        .collect();
    for (field, at, _) in fields {
        match field.effect {
            FieldEffect::Strain => {
                for &k in &bonds {
                    let w = field.weight(at, world_point(p.bond_centre[k]));
                    f.damage[k] += field.magnitude * w * dt;
                }
            }
            FieldEffect::Crumble => {
                if f.pieces
                    .iter()
                    .any(|&q| field.weight(at, world_point(p.centre(q))) > 0.0)
                {
                    for &k in &bonds {
                        let due = breaking_damage(p.loaded.structure.edges[k].strength);
                        f.damage[k] = f.damage[k].max(due);
                    }
                }
            }
            FieldEffect::Anchor => {
                for &q in &f.pieces {
                    if field.weight(at, world_point(p.centre(q))) > 0.0 {
                        let cell = p.loaded.fracture.pieces[q as usize].cell;
                        if let Err(i) = f.anchors.binary_search(&cell) {
                            f.anchors.insert(i, cell);
                        }
                    }
                }
            }
            FieldEffect::Kill => {
                let (_, centre) = p.mass_and_centre(&f.pieces);
                if debris && field.weight(at, world_point(centre)) > 0.0 {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(f)
}

/// The least damage that breaks a bond of `strength` (bonds break once damage, widened to
/// `f64`, reaches it).
pub(crate) fn breaking_damage(strength: f64) -> f32 {
    let nearest = strength as f32;
    if f64::from(nearest) < strength {
        nearest.next_up()
    } else {
        nearest
    }
}

/// What fields do to a fragment's body this tick.
#[derive(Default)]
struct Pushes {
    impulse: Vec3,
    sleep: bool,
    disable: bool,
}

/// The fragment's pushes from force, impulse, sleep and disable fields, at its centre of mass.
fn pushes(p: &Prepared, fields: &[Acting], f: &Fragment, t: &Transform, dt: f32) -> Pushes {
    let (_, centre) = p.mass_and_centre(&f.pieces);
    let com = t.translation + t.rotation * centre;
    let mut out = Pushes::default();
    for (field, at, first) in fields {
        let w = field.weight(at, com);
        if w <= 0.0 {
            continue;
        }
        let away = match &field.shape {
            FieldShape::Plane { normal } => (at.rotation * *normal).normalize_or_zero(),
            _ => (com - at.translation).normalize_or_zero(),
        };
        match field.effect {
            FieldEffect::Force => out.impulse += away * field.magnitude * w * dt,
            FieldEffect::Impulse if *first => out.impulse += away * field.magnitude * w,
            FieldEffect::Sleep => out.sleep = true,
            FieldEffect::Disable => out.disable = true,
            _ => {}
        }
    }
    out
}

impl Pushes {
    fn apply(self, world: &mut World, id: EntityId) {
        if self.impulse != Vec3::ZERO {
            let mut impulse = world.get::<Impulse>(id).copied().unwrap_or_default();
            impulse.linear += self.impulse;
            let _ = world.insert(id, impulse);
        }
        if self.sleep {
            let _ = world.insert(id, Sleep {});
        }
        let disabled = world.get::<PhysicsDisabled>(id).is_some();
        if self.disable && !disabled {
            let _ = world.insert(id, PhysicsDisabled {});
        } else if !self.disable && disabled {
            let _ = world.remove::<PhysicsDisabled>(id);
        }
    }
}

/// Makes a newly anchored fragment static and still.
fn pin(world: &mut World, id: EntityId) {
    if let Some(mut body) = world.get::<RigidBody>(id).cloned() {
        body.kind = BodyKind::Static;
        let _ = world.insert(id, body);
    }
    let _ = world.insert(id, Velocity::default());
}

/// Gives `id` the fragment's body: a compound of its pieces with their mass, static if anchored.
fn attach(
    world: &mut World,
    id: EntityId,
    p: &Prepared,
    fragment: Fragment,
    mut body: RigidBody,
) -> Result<(), concordia_terra::EcsError> {
    let (mass, _) = p.mass_and_centre(&fragment.pieces);
    body.mass = Some(mass as f32);
    if anchored(p, &fragment) {
        body.kind = BodyKind::Static;
    }
    world.insert(
        id,
        Collider::new(ColliderShape::Compound {
            parts: p.hulls(&fragment.pieces),
        }),
    )?;
    world.insert(id, body)?;
    world.insert(id, ContactReports { min_strength: 0.0 })?;
    world.insert(id, fragment).map(drop)
}

fn anchored(p: &Prepared, fragment: &Fragment) -> bool {
    fragment.anchors.iter().any(|cell| {
        p.piece_of_cell
            .get(cell)
            .is_some_and(|piece| fragment.pieces.binary_search(piece).is_ok())
    })
}

/// Replaces `id` by one entity per connected part of its pieces (or keeps it if still whole).
fn split(
    world: &mut World,
    id: EntityId,
    p: &Prepared,
    fragment: Fragment,
) -> Result<(), concordia_terra::EcsError> {
    let parts = components(p, &fragment);
    if parts.len() < 2 {
        return world.insert(id, fragment).map(drop);
    }
    let t = world.get::<Transform>(id).copied().unwrap_or_default();
    let v = world.get::<Velocity>(id).copied().unwrap_or_default();
    let body = world
        .get::<RigidBody>(id)
        .cloned()
        .unwrap_or_else(RigidBody::dynamic);
    let (_, whole) = p.mass_and_centre(&fragment.pieces);
    let drawn = world.get::<concordia_assets::PieceRenderer>(id).cloned();
    world.despawn(id)?;
    for pieces in parts {
        let part = Fragment {
            pieces,
            ..fragment.clone()
        };
        let child = child_id(id, part.pieces[0]);
        world.spawn_with_id(child)?;
        world.insert(child, t)?;
        crate::looks::draw_part(world, child, drawn.as_ref(), &part.pieces)?;
        let (_, centre) = p.mass_and_centre(&part.pieces);
        let still = anchored(p, &part);
        let velocity = match still {
            true => Velocity::default(),
            // The part moves as it did inside the whole: v + ω × r at its centre of mass.
            false => Velocity {
                linear: v.linear + v.angular.cross(t.rotation * (centre - whole)),
                angular: v.angular,
            },
        };
        world.insert(child, velocity)?;
        let kind = if still {
            BodyKind::Static
        } else {
            BodyKind::Dynamic
        };
        attach(
            world,
            child,
            p,
            part,
            RigidBody {
                kind,
                ..body.clone()
            },
        )?;
    }
    Ok(())
}

/// The fragment's pieces in connected groups over its unbroken bonds, ordered by lowest piece.
fn components(p: &Prepared, fragment: &Fragment) -> Vec<Vec<u32>> {
    let index: BTreeMap<u32, usize> = fragment
        .pieces
        .iter()
        .enumerate()
        .map(|(i, &piece)| (piece, i))
        .collect();
    let mut parent: Vec<usize> = (0..fragment.pieces.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for k in p.internal_bonds(&fragment.pieces) {
        if fragment.broken.binary_search(&(k as u32)).is_ok() {
            continue;
        }
        let (a, b) = p.bond_pieces[k];
        let (ra, rb) = (find(&mut parent, index[&a]), find(&mut parent, index[&b]));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut groups: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for (i, &piece) in fragment.pieces.iter().enumerate() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(piece);
    }
    let mut out: Vec<Vec<u32>> = groups.into_values().collect();
    out.sort_by_key(|g| g[0]);
    out
}

/// A part's id: derived from its parent and its lowest piece, so it is the same on every run.
fn child_id(parent: EntityId, lowest: u32) -> EntityId {
    let mut name = b"fragment:".to_vec();
    name.extend(lowest.to_le_bytes());
    let uuid = uuid::Uuid::new_v5(&parent.as_uuid(), &name);
    EntityId::try_from_uuid(uuid).unwrap_or(parent)
}

#[cfg(test)]
mod tests {
    use super::breaking_damage;

    #[test]
    fn breaking_damage_reaches_the_strength_however_it_rounds() {
        for strength in [
            100.0,
            20_000.000_000_004,
            0.1,
            1.0e-3 + 1.0e-12,
            3.3e7 + 0.5,
        ] {
            let damage = breaking_damage(strength);
            assert!(f64::from(damage) >= strength, "{strength}: {damage}");
            assert!(
                f64::from(damage) - strength <= strength * 1.0e-6,
                "and barely above"
            );
        }
    }
}
