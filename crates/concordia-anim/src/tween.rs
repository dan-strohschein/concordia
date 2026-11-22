//! Property tweens (M2 phase 2.4.4, Godot's `Tween` transitions and eases): a numeric field of any
//! reflected component, eased toward a target at the scaled fixed step (hashed simulation state).

use std::collections::BTreeMap;

use concordia_reflect::{Reflect, TypeRegistry};
use concordia_terra::{JsonComponent, Phase, Schedule, SystemCtx, World, system};
use serde::{Deserialize, Serialize};

/// `anim.tween`: advances every entity's tweens (an exclusive system, before `anim.pose`).
pub const TWEEN: &str = "anim.tween";

/// The tweens animating this entity's fields; a tween dies with its entity.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Animation", default)]
#[serde(deny_unknown_fields)]
pub struct Tweens {
    /// Running tweens, applied in order (on one field, the last wins); finished ones are removed.
    pub tweens: Vec<PropertyTween>,
}

/// One field eased from its value toward `to`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PropertyTween {
    /// The component's name, e.g. "Transform".
    pub component: String,
    /// JSON pointer to a number or an array of numbers in it, e.g. "/translation".
    pub field: String,
    /// Target value (one number per element).
    pub to: Vec<f32>,
    /// Start value; captured from the field when the delay ends, unless given.
    #[serde(default)]
    pub from: Option<Vec<f32>>,
    /// Seconds from start to target (0 jumps).
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub duration: f32,
    /// Seconds to wait before starting.
    #[serde(default)]
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub delay: f32,
    /// Seconds since the tween was added (delay included).
    #[serde(default)]
    pub elapsed: f32,
    /// The curve's shape (default linear).
    #[serde(default)]
    pub transition: Transition,
    /// Which end the shape applies to (default in-out).
    #[serde(default)]
    pub ease: Ease,
}

/// A tween curve's shape (Godot's `Tween.TransitionType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Transition {
    /// Constant speed.
    #[default]
    Linear,
    /// A quarter sine wave.
    Sine,
    /// t⁵.
    Quint,
    /// t⁴.
    Quart,
    /// t².
    Quad,
    /// 2^(10(t − 1)).
    Expo,
    /// A spring that overshoots and settles.
    Elastic,
    /// t³.
    Cubic,
    /// A quarter circle.
    Circ,
    /// Bounces at the end.
    Bounce,
    /// Pulls back before going.
    Back,
    /// Godot's damped spring.
    Spring,
}

/// Which end of the curve the shape applies to (Godot's `Tween.EaseType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Ease {
    /// Slow start.
    In,
    /// Slow end.
    Out,
    /// Slow at both ends.
    #[default]
    InOut,
    /// Fast at both ends.
    OutIn,
}

/// Progress at `x` (0..=1 of the duration) for `transition` and `ease`: 0 at 0, 1 at 1 (Godot's
/// easing equations, after Robert Penner's; `x` is clamped, NaN is 0).
pub fn ease(transition: Transition, ease: Ease, x: f32) -> f32 {
    let x = if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) };
    if transition == Transition::Linear {
        return x;
    }
    match ease {
        Ease::In => ease_in(transition, x),
        Ease::Out => ease_out(transition, x),
        Ease::InOut => ease_in_out(transition, x),
        Ease::OutIn => {
            if x < 0.5 {
                ease_out(transition, 2.0 * x) / 2.0
            } else {
                0.5 + ease_in(transition, 2.0 * x - 1.0) / 2.0
            }
        }
    }
}

const BACK: f32 = 1.70158;

fn ease_in(t: Transition, x: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, TAU};
    match t {
        Transition::Linear => x,
        Transition::Sine => 1.0 - (x * FRAC_PI_2).cos(),
        Transition::Quint => x.powi(5),
        Transition::Quart => x.powi(4),
        Transition::Quad => x * x,
        Transition::Expo if x == 0.0 => 0.0,
        Transition::Expo => 2f32.powf(10.0 * (x - 1.0)) - 0.001,
        Transition::Elastic if x == 0.0 || x == 1.0 => x,
        Transition::Elastic => {
            let (p, v) = (0.3, x - 1.0);
            -(2f32.powf(10.0 * v) * ((v - p / 4.0) * TAU / p).sin())
        }
        Transition::Cubic => x.powi(3),
        Transition::Circ => 1.0 - (1.0 - x * x).sqrt(),
        Transition::Bounce => 1.0 - bounce_out(1.0 - x),
        Transition::Back => x * x * ((BACK + 1.0) * x - BACK),
        Transition::Spring => 1.0 - spring_out(1.0 - x),
    }
}

fn ease_out(t: Transition, x: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, TAU};
    match t {
        Transition::Linear => x,
        Transition::Sine => (x * FRAC_PI_2).sin(),
        Transition::Quint => (x - 1.0).powi(5) + 1.0,
        Transition::Quart => 1.0 - (x - 1.0).powi(4),
        Transition::Quad => -x * (x - 2.0),
        Transition::Expo if x == 1.0 => 1.0,
        Transition::Expo => 1.001 * (1.0 - 2f32.powf(-10.0 * x)),
        Transition::Elastic if x == 0.0 || x == 1.0 => x,
        Transition::Elastic => {
            let p = 0.3;
            2f32.powf(-10.0 * x) * ((x - p / 4.0) * TAU / p).sin() + 1.0
        }
        Transition::Cubic => (x - 1.0).powi(3) + 1.0,
        Transition::Circ => (1.0 - (x - 1.0).powi(2)).sqrt(),
        Transition::Bounce => bounce_out(x),
        Transition::Back => {
            let u = x - 1.0;
            u * u * ((BACK + 1.0) * u + BACK) + 1.0
        }
        Transition::Spring => spring_out(x),
    }
}

/// In for the first half, out for the second; expo, elastic and back use Godot's own forms.
fn ease_in_out(t: Transition, x: f32) -> f32 {
    use std::f32::consts::TAU;
    let u = 2.0 * x;
    match t {
        Transition::Expo if x == 0.0 || x == 1.0 => x,
        Transition::Expo if u < 1.0 => 2f32.powf(10.0 * (u - 1.0)) / 2.0 - 0.0005,
        Transition::Expo => 0.5 * 1.0005 * (2.0 - 2f32.powf(-10.0 * (u - 1.0))),
        Transition::Elastic if x == 0.0 || x == 1.0 => x,
        Transition::Elastic => {
            let (p, v) = (0.45, u - 1.0);
            let wave = ((v - p / 4.0) * TAU / p).sin();
            if u < 1.0 {
                -0.5 * 2f32.powf(10.0 * v) * wave
            } else {
                0.5 * 2f32.powf(-10.0 * v) * wave + 1.0
            }
        }
        Transition::Back => {
            let s = BACK * 1.525;
            if u < 1.0 {
                u * u * ((s + 1.0) * u - s) / 2.0
            } else {
                let v = u - 2.0;
                (v * v * ((s + 1.0) * v + s) + 2.0) / 2.0
            }
        }
        _ if x < 0.5 => ease_in(t, u) / 2.0,
        _ => 0.5 + ease_out(t, u - 1.0) / 2.0,
    }
}

fn bounce_out(x: f32) -> f32 {
    const K: f32 = 7.5625;
    if x < 1.0 / 2.75 {
        K * x * x
    } else if x < 2.0 / 2.75 {
        let u = x - 1.5 / 2.75;
        K * u * u + 0.75
    } else if x < 2.5 / 2.75 {
        let u = x - 2.25 / 2.75;
        K * u * u + 0.9375
    } else {
        let u = x - 2.625 / 2.75;
        K * u * u + 0.984_375
    }
}

/// Godot's damped spring.
fn spring_out(x: f32) -> f32 {
    let s = 1.0 - x;
    ((x * std::f32::consts::PI * (0.2 + 2.5 * x.powi(3))).sin() * s.powf(2.2) + x) * (1.0 + 1.2 * s)
}

/// Adds `anim.tween` to `FrameEnd`; every component in `registry` can be tweened.
pub fn add_tweens(schedule: &mut Schedule, registry: &TypeRegistry) {
    let codecs: BTreeMap<&'static str, JsonComponent> = registry
        .names()
        .filter_map(|name| JsonComponent::of(registry, name).ok().map(|c| (name, c)))
        .collect();
    schedule.add_in(
        Phase::FrameEnd,
        system(TWEEN, move |world: &mut World, ctx: &SystemCtx<'_>| {
            tick_tweens(&codecs, world, ctx.dt);
        }),
    );
}

type Codecs = BTreeMap<&'static str, JsonComponent>;

/// Advances every entity's tweens by `dt`, in entity order; finished and broken ones are removed.
fn tick_tweens(codecs: &Codecs, world: &mut World, dt: f32) {
    let mut ids = Vec::new();
    if let Ok(mut query) = world.query::<&Tweens>() {
        query.for_each(world, |id, _| ids.push(id));
    }
    ids.sort_unstable();
    for id in ids {
        let Some(Tweens { tweens }) = world.get::<Tweens>(id).cloned() else {
            continue;
        };
        let mut running = Vec::with_capacity(tweens.len());
        for mut tween in tweens {
            match step(codecs, world, id, &mut tween, dt) {
                Ok(false) => running.push(tween),
                Ok(true) => {}
                Err(reason) => {
                    tracing::warn!(target: "concordia::anim", entity = %id, component = tween.component, field = tween.field, reason, "tween dropped")
                }
            }
        }
        let written = if running.is_empty() {
            world.remove::<Tweens>(id).map(drop)
        } else {
            world.insert(id, Tweens { tweens: running }).map(drop)
        };
        debug_assert!(written.is_ok(), "the entity was just queried");
    }
}

/// Moves one tween `dt` on and writes its field; `Ok(true)` once it has reached its target.
fn step(
    codecs: &Codecs,
    world: &mut World,
    id: concordia_core::id::EntityId,
    tween: &mut PropertyTween,
    dt: f32,
) -> Result<bool, String> {
    let timing = [tween.duration, tween.delay, tween.elapsed + dt];
    if timing.iter().any(|v| !v.is_finite() || *v < 0.0) || tween.to.iter().any(|v| !v.is_finite())
    {
        return Err("durations, delays and targets must be finite and non-negative".to_owned());
    }
    tween.elapsed += dt;
    if tween.elapsed < tween.delay {
        return Ok(false);
    }
    let codec = codecs
        .get(tween.component.as_str())
        .ok_or("no such component type")?;
    let mut json = world
        .get_json(id, &tween.component)
        .map_err(|e| e.to_string())?;
    let field = json
        .pointer_mut(&tween.field)
        .ok_or("no such field in the component")?;
    let current = numbers(field).ok_or("the field is not a number or an array of numbers")?;
    if current.len() != tween.to.len() {
        return Err(format!(
            "the field has {} numbers, the target {}",
            current.len(),
            tween.to.len()
        ));
    }
    let from = tween.from.get_or_insert(current).clone();
    if from.len() != tween.to.len() {
        return Err("the start and the target differ in length".to_owned());
    }
    let x = match tween.duration > 0.0 {
        true => ((tween.elapsed - tween.delay) / tween.duration).min(1.0),
        false => 1.0,
    };
    let done = x >= 1.0;
    let mut value: Vec<f32> = match done {
        true => tween.to.clone(),
        false => {
            let k = ease(tween.transition, tween.ease, x);
            from.iter()
                .zip(&tween.to)
                .map(|(a, b)| a + (b - a) * k)
                .collect()
        }
    };
    if value.len() == 4 && tween.field.ends_with("/rotation") {
        normalize(&mut value);
    }
    put(field, &value);
    codec.set(world, id, json).map_err(|e| e.to_string())?;
    Ok(done)
}

/// A JSON number or array of numbers as floats.
fn numbers(value: &serde_json::Value) -> Option<Vec<f32>> {
    match value {
        serde_json::Value::Number(n) => Some(vec![n.as_f64()? as f32]),
        serde_json::Value::Array(items) => items.iter().map(|v| Some(v.as_f64()? as f32)).collect(),
        _ => None,
    }
}

/// Writes `value` into `field`, keeping its shape (a number or an array).
fn put(field: &mut serde_json::Value, value: &[f32]) {
    let number = |v: f32| serde_json::Value::from(f64::from(v));
    *field = match field {
        serde_json::Value::Number(_) => number(value[0]),
        _ => serde_json::Value::Array(value.iter().map(|v| number(*v)).collect()),
    };
}

/// Scales a quaternion to unit length (a zero one is left alone).
fn normalize(q: &mut [f32]) {
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if n > 0.0 {
        q.iter_mut().for_each(|v| *v /= n);
    }
}
