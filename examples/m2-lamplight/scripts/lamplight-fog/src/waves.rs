//! The fog's processors: waves of wisps from the harbour while the night is played, each aimed
//! at a point near the square.

use concordia_core::math::Vec3;
use concordia_script_guest::RunCtx;
use lamplight_types::engine::{NavAgent, Transform};
use lamplight_types::night::{NewNight, NightTick};

use crate::{Drifter, Fog};

/// Counts down to each wave and sends it: `per_wave × density` wisps (at least one) at random
/// points on the harbour edge, each drifting to a random point on the ring `inner..outer` around
/// the square.
pub(crate) fn waves(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    if !ctx.signals::<NewNight>()?.is_empty() {
        let mut fogs: Vec<Fog> = ctx.read()?;
        for fog in &mut fogs {
            fog.next = fog.first;
            fog.waves = 0;
        }
        return ctx.write(&fogs);
    }
    let Some((_, night)) = ctx.signals::<NightTick>()?.into_iter().last() else {
        return Ok(());
    };
    if !night.playing() {
        return Ok(());
    }
    let dt = ctx.dt();
    let mut fogs: Vec<Fog> = ctx.read()?;
    for fog in &mut fogs {
        fog.next -= dt;
        if fog.next > 0.0 {
            continue;
        }
        fog.next += fog.every;
        fog.waves += 1;
        let count = ((fog.per_wave as f32 * night.density).round() as u32).max(1);
        for _ in 0..count {
            let at = fog.origin
                + Vec3::new(
                    (ctx.rng().next_f32() * 2.0 - 1.0) * fog.half_extents.x,
                    0.0,
                    (ctx.rng().next_f32() * 2.0 - 1.0) * fog.half_extents.z,
                );
            let target = fog.square + ring(ctx, fog.inner, fog.outer);
            let wisp = ctx.spawn_template("wisp");
            ctx.insert(wisp, &Transform::at(at))?;
            ctx.insert(wisp, &Drifter { target })?;
        }
    }
    ctx.write(&fogs)
}

/// A random point on the ground between `inner` and `outer` from the origin: a random direction
/// (by rejection in the unit disc) at a random distance in the band.
fn ring(ctx: &mut RunCtx<'_>, inner: f32, outer: f32) -> Vec3 {
    let (x, z) = loop {
        let x = ctx.rng().next_f32() * 2.0 - 1.0;
        let z = ctx.rng().next_f32() * 2.0 - 1.0;
        let length_sq = x * x + z * z;
        if length_sq > 1e-4 && length_sq <= 1.0 {
            let length = length_sq.sqrt();
            break (x / length, z / length);
        }
    };
    let distance = inner + ctx.rng().next_f32() * (outer - inner).max(0.0);
    Vec3::new(x * distance, 0.0, z * distance)
}

/// Gives every drifter without a target its drift target.
pub(crate) fn steer(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let drifters: Vec<Drifter> = ctx.read()?;
    let mut agents: Vec<NavAgent> = ctx.read()?;
    for (agent, drifter) in agents.iter_mut().zip(&drifters) {
        if agent.target.is_none() {
            agent.target = Some(drifter.target);
        }
    }
    ctx.write(&agents)
}
