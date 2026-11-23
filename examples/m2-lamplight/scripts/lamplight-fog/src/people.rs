//! The townsfolk's processors: wisps say where they are, and townsfolk stroll between home and
//! the market, flee home from wisps, and update only when their simulation LOD is due.

use concordia_core::math::{Quat, Vec3};
use concordia_script_guest::math::{atan2f, cosf, sinf};
use concordia_script_guest::{EntityId, RunCtx};
use lamplight_types::body::Stride;
use lamplight_types::engine::{NavAgent, NavArrived, SimLod, Transform};

use crate::townsfolk::{CALM_SECONDS, FLEE_SPEED, STROLL_SPEED, frightened, walk_amount};
use crate::{Townsperson, WispAt};

/// How close to their door a frightened townsperson counts as home (m).
const AT_HOME: f32 = 1.0;

/// Every wisp says where it is (heard next tick).
pub(crate) fn watch(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let transforms: Vec<Transform> = ctx.read()?;
    for (id, t) in ids.iter().zip(&transforms) {
        ctx.signal(*id, &WispAt { at: t.translation })?;
    }
    Ok(())
}

fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    let (dx, dz) = (a.x - b.x, a.z - b.z);
    (dx * dx + dz * dz).sqrt()
}

/// Townsfolk, each when due: face the way they walk, blend the walk, take fright at a close
/// wisp (fleeing home at a run and startling), calm down at home, and otherwise stroll between
/// home and the market, turning back at each arrival.
pub(crate) fn townsfolk(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let wisps: Vec<Vec3> = ctx
        .signals::<WispAt>()?
        .into_iter()
        .map(|(_, w)| w.at)
        .collect();
    let arrived: Vec<EntityId> = ctx
        .signals::<NavArrived>()?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let lods: Vec<SimLod> = ctx.read()?;
    let mut people: Vec<Townsperson> = ctx.read()?;
    let mut agents: Vec<NavAgent> = ctx.read()?;
    let mut transforms: Vec<Transform> = ctx.read()?;
    for i in 0..ids.len() {
        if !lods[i].due {
            continue;
        }
        let dt = lods[i].elapsed as f32;
        let p = &mut people[i];
        p.updates += 1;
        let now = transforms[i].translation;
        let moved = flat_distance(now, p.last);
        if moved > 1e-3 {
            let angle = atan2f(now.x - p.last.x, now.z - p.last.z);
            transforms[i].rotation =
                Quat::from_xyzw(0.0, sinf(angle / 2.0), 0.0, cosf(angle / 2.0));
        }
        p.last = now;
        let mut startle = false;
        if !p.scared && frightened(now, &wisps) {
            p.scared = true;
            p.calm_in = CALM_SECONDS;
            startle = true;
        } else if p.scared && flat_distance(now, p.home) <= AT_HOME {
            p.calm_in -= dt;
            if p.calm_in <= 0.0 {
                p.scared = false;
                p.homeward = false;
            }
        } else if !p.scared && arrived.contains(&ids[i]) {
            p.homeward = !p.homeward;
        }
        let target = if p.scared || p.homeward {
            p.home
        } else {
            p.market
        };
        if agents[i].target != Some(target) {
            agents[i].target = Some(target);
        }
        agents[i].max_speed = if p.scared { FLEE_SPEED } else { STROLL_SPEED };
        if let Some(body) = p.body {
            let stride = Stride {
                walk_amount: walk_amount(moved, dt),
                light: false,
                startle,
            };
            ctx.signal(body, &stride)?;
        }
    }
    ctx.write(&people)?;
    ctx.write(&agents)?;
    ctx.write(&transforms)
}
