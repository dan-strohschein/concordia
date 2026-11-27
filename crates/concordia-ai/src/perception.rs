//! Perception (M4 4.12c, M8): sight (cone, range, line of sight through physics), hearing
//! (`ai.Noise`, linear falloff), damage (`ai.Damaged`), and a memory that decays and forgets.
//! The strongest remembered hostile goes to the blackboard's `target` / `target_position` keys.

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_core::math::{Ray, Vec3};
use concordia_physics::{LayerMask, PhysicsQueries};
use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use concordia_terra::{Phase, Schedule, SystemCtx, View, processor};
use serde::{Deserialize, Serialize};

use crate::engine::THINK;
use crate::{AiConfig, BbType, BbValue, Blackboard};

/// The `ai.sense` processor's name.
pub const SENSE: &str = "ai.sense";

/// What an agent senses with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ai.Perceiver", category = "AI", default)]
pub struct Perceiver {
    /// Metres it sees (0: blind).
    pub sight_range: f32,
    /// Degrees of its view cone (around its facing, +Z).
    pub sight_fov: f32,
    /// Metres it hears.
    pub hearing_range: f32,
    /// Seconds a full-strength memory lasts.
    pub memory_seconds: f32,
    /// Height of its eyes and of what it looks at.
    pub eye_height: f32,
    /// Factions it notices (empty: every faction but its own).
    pub notices: Vec<String>,
}

impl Default for Perceiver {
    fn default() -> Self {
        Self {
            sight_range: 15.0,
            sight_fov: 120.0,
            hearing_range: 20.0,
            memory_seconds: 5.0,
            eye_height: 1.6,
            notices: Vec::new(),
        }
    }
}

/// Which side an entity is on.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "ai.Faction", category = "AI", default)]
pub struct Faction {
    /// Its name.
    pub name: String,
}

/// How something was sensed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Sense {
    /// Seen.
    #[default]
    Sight,
    /// Heard.
    Hearing,
    /// Hurt by it.
    Damage,
}

/// One remembered entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Remembered {
    /// The entity.
    pub entity: EntityId,
    /// Where it was last sensed.
    pub position: [f32; 3],
    /// How.
    pub sense: Sense,
    /// 1 when just sensed, falling to 0 over `memory_seconds`.
    pub strength: f32,
    /// The tick it was last sensed.
    pub last_tick: u64,
}

/// What an agent remembers sensing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "ai.Memory", category = "AI", default)]
pub struct Memory {
    /// Remembered entities, by entity id.
    pub entries: Vec<Remembered>,
}

/// Signal on a noise's source: heard within `radius`, `loudness` falling linearly to 0 at it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, name = "ai.Noise", category = "AI", default)]
pub struct AiNoise {
    /// Where.
    pub position: [f32; 3],
    /// How far it carries.
    pub radius: f32,
    /// How loud at its source (0 to 1).
    pub loudness: f32,
}

/// Signal on a victim: `source` hurt it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal, name = "ai.Damaged", category = "AI")]
pub struct AiDamaged {
    /// Who.
    pub source: EntityId,
    /// How much.
    pub amount: f32,
}

/// Where the sight rotation resumes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SenseCursor(pub usize);

/// Adds `ai.sense` to `PostPhysics`, before `ai.think`.
pub fn add_perception(schedule: &mut Schedule, config: AiConfig) {
    schedule.add_processor(
        processor(SENSE, Phase::PostPhysics)
            .reads::<Perceiver>()
            .reads::<Faction>()
            .reads::<Transform>()
            .writes::<Memory>()
            .writes::<Blackboard>()
            .receives::<AiNoise>()
            .receives::<AiDamaged>()
            .reads_resource::<PhysicsQueries>()
            .writes_resource::<SenseCursor>()
            .before(THINK)
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| sense(view, ctx, config)),
    );
}

/// A sensing this tick.
struct Sensed {
    entity: EntityId,
    position: Vec3,
    sense: Sense,
    strength: f32,
}

/// Everything perception reads this tick.
struct World2 {
    factions: BTreeMap<EntityId, String>,
    transforms: BTreeMap<EntityId, Transform>,
    noises: Vec<(EntityId, AiNoise)>,
    damage: Vec<(EntityId, AiDamaged)>,
    physics: Option<PhysicsQueries>,
}

fn sense(view: &mut View<'_>, ctx: &SystemCtx<'_>, config: AiConfig) {
    let mut w = World2 {
        factions: BTreeMap::new(),
        transforms: BTreeMap::new(),
        noises: view
            .signals::<AiNoise>()
            .map(<[_]>::to_vec)
            .unwrap_or_default(),
        damage: view
            .signals::<AiDamaged>()
            .map(<[_]>::to_vec)
            .unwrap_or_default(),
        physics: view.resource::<PhysicsQueries>().ok().cloned(),
    };
    if let Ok(q) = view.query::<&Faction>() {
        q.for_each(|id, f| {
            w.factions.insert(id, f.name.clone());
        });
    }
    if let Ok(q) = view.query::<&Transform>() {
        q.for_each(|id, t| {
            w.transforms.insert(id, *t);
        });
    }
    let mut perceivers: Vec<(EntityId, Perceiver)> = Vec::new();
    if let Ok(q) = view.query::<&Perceiver>() {
        q.for_each(|id, p| perceivers.push((id, p.clone())));
    }
    perceivers.sort_by_key(|(id, _)| *id);
    let looking = rotation(view, perceivers.len(), config.sight_per_tick as usize);
    let sensed: BTreeMap<EntityId, Vec<Sensed>> = perceivers
        .iter()
        .enumerate()
        .map(|(k, (id, p))| (*id, sense_one(&w, *id, p, looking(k))))
        .collect();
    let memory_seconds: BTreeMap<EntityId, f32> = perceivers
        .iter()
        .map(|(id, p)| (*id, p.memory_seconds))
        .collect();
    let tick = ctx.tick.0;
    if let Ok(q) = view.query::<(&mut Memory, &mut Blackboard)>() {
        q.for_each(|id, (memory, bb)| {
            let Some(seconds) = memory_seconds.get(&id) else {
                return;
            };
            remember(
                memory,
                sensed.get(&id).map_or(&[][..], Vec::as_slice),
                ctx.dt / seconds.max(1e-3),
                tick,
            );
            to_blackboard(memory, bb);
        });
    }
}

/// Which perceivers look this tick (a rotation of `per_tick`; the rest only hear and feel).
fn rotation(view: &mut View<'_>, n: usize, per_tick: usize) -> impl Fn(usize) -> bool + use<> {
    let start = view.resource::<SenseCursor>().map_or(0, |c| c.0);
    let take = per_tick.min(n);
    if let Ok(c) = view.resource_mut::<SenseCursor>() {
        c.0 = if n == 0 { 0 } else { (start + take) % n };
    }
    move |k| n > 0 && (k + n - start) % n < take
}

fn hostile(w: &World2, me: EntityId, p: &Perceiver, other: EntityId) -> bool {
    let Some(theirs) = w.factions.get(&other) else {
        return false;
    };
    match p.notices.is_empty() {
        true => w.factions.get(&me) != Some(theirs),
        false => p.notices.contains(theirs),
    }
}

fn sense_one(w: &World2, me: EntityId, p: &Perceiver, look: bool) -> Vec<Sensed> {
    let Some(own) = w.transforms.get(&me) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if look && p.sight_range > 0.0 {
        let eye = own.translation + Vec3::Y * p.eye_height;
        let forward = own.rotation * Vec3::Z;
        let cos_half = (p.sight_fov.to_radians() / 2.0).cos();
        for (&other, t) in &w.transforms {
            if other == me || !hostile(w, me, p, other) {
                continue;
            }
            let target = t.translation + Vec3::Y * p.eye_height;
            let to = target - eye;
            let distance = to.length();
            if distance > p.sight_range || distance < 1e-4 || forward.dot(to / distance) < cos_half
            {
                continue;
            }
            if blocked(w.physics.as_ref(), eye, to / distance, distance, other) {
                continue;
            }
            out.push(Sensed {
                entity: other,
                position: t.translation,
                sense: Sense::Sight,
                strength: 1.0,
            });
        }
    }
    for (source, noise) in &w.noises {
        if *source == me || (w.factions.contains_key(source) && !hostile(w, me, p, *source)) {
            continue;
        }
        let at = Vec3::from_array(noise.position);
        let distance = (at - own.translation).length();
        if noise.radius > 0.0 && distance <= noise.radius.min(p.hearing_range) {
            let strength = (noise.loudness * (1.0 - distance / noise.radius)).clamp(0.0, 1.0);
            out.push(Sensed {
                entity: *source,
                position: at,
                sense: Sense::Hearing,
                strength,
            });
        }
    }
    for (victim, hurt) in &w.damage {
        if *victim == me {
            let position = w
                .transforms
                .get(&hurt.source)
                .map_or(own.translation, |t| t.translation);
            out.push(Sensed {
                entity: hurt.source,
                position,
                sense: Sense::Damage,
                strength: 1.0,
            });
        }
    }
    out
}

/// Whether something other than `target` stands in the way (no physics: nothing does).
fn blocked(
    physics: Option<&PhysicsQueries>,
    eye: Vec3,
    dir: Vec3,
    distance: f32,
    target: EntityId,
) -> bool {
    let (Some(physics), Ok(ray)) = (physics, Ray::new(eye, dir)) else {
        return false;
    };
    physics
        .raycast(&ray, distance, LayerMask::ALL)
        .is_some_and(|hit| hit.entity != target && hit.distance < distance - 0.05)
}

/// Decays every memory by `fade`, forgets the faded, then takes this tick's sensings.
fn remember(memory: &mut Memory, sensed: &[Sensed], fade: f32, tick: u64) {
    for e in &mut memory.entries {
        e.strength -= fade;
    }
    memory.entries.retain(|e| e.strength > 1e-4);
    for s in sensed {
        let fresh = Remembered {
            entity: s.entity,
            position: s.position.to_array(),
            sense: s.sense,
            strength: s.strength,
            last_tick: tick,
        };
        match memory.entries.iter_mut().find(|e| e.entity == s.entity) {
            Some(e) if e.last_tick == tick && e.strength >= s.strength => {}
            Some(e) => {
                *e = Remembered {
                    strength: s
                        .strength
                        .max(if e.last_tick == tick { e.strength } else { 0.0 }),
                    ..fresh
                }
            }
            None => memory.entries.push(fresh),
        }
    }
    memory.entries.sort_by_key(|e| e.entity);
}

/// The strongest memory into `target` / `target_position`, where the tree declares them.
fn to_blackboard(memory: &Memory, bb: &mut Blackboard) {
    let best = memory
        .entries
        .iter()
        .fold(None::<&Remembered>, |best, e| match best {
            Some(b) if b.strength >= e.strength => Some(b),
            _ => Some(e),
        });
    if bb.keys.get("target") == Some(&BbType::Entity) {
        let _ = bb.set(
            "target",
            best.map_or(BbValue::None, |e| BbValue::Entity(e.entity)),
        );
    }
    if bb.keys.get("target_position") == Some(&BbType::Vec3) {
        let _ = bb.set(
            "target_position",
            best.map_or(BbValue::None, |e| BbValue::Vec3(e.position)),
        );
    }
}
