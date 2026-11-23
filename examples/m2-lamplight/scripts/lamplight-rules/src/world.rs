//! The processors: the pure rules applied to the world. Each reads whole columns, changes them,
//! and writes them back; facts about other entities arrive as signals (delivered a tick later).

use concordia_core::math::{Quat, Vec3};
use concordia_script_guest::math::{atan2f, cosf, sinf};
use concordia_script_guest::{EntityId, RunCtx};
use lamplight_types::engine::{
    AnimEvent, AnimGraphPlayer, AudioEmitter, CharacterMotion, ContactBegan, ContactEnded, Ease,
    InputActions, NavArrived, PlayCue, PointLight, PropertyTween, Transform, Transition, Tweens,
    UiText,
};
use lamplight_types::night::{self, NightTick};

use crate::rules::{self, Outcome};
use crate::{
    Body, Chase, Kindle, Lamp, Lamplighter, Lit, Night, Number, Popped, Reached, Scorch,
    SpiritLost, Stride, Where, Wisp,
};

/// The graph parameter the walk blend reads.
const WALK_AMOUNT: &str = "walk_amount";
/// The trigger that fires the Light one-shot.
const LIGHT_NOW: &str = "light_now";
/// The clip event at the top of the reach.
const LIT_EVENT: &str = "lit";
/// How high above a wisp its numbers start, and how far they rise (m).
const NUMBER_HEIGHT: f32 = 2.2;
const NUMBER_RISE: f32 = 1.5;
/// How long a number lasts (ticks at 60 Hz) and its rise (s).
const NUMBER_TICKS: u32 = 60;
const NUMBER_SECONDS: f32 = 1.0;
/// Spirit numbers rise over the fountain from this high (m).
const SPIRIT_HEIGHT: f32 = 4.0;

/// The row of `id` in this batch.
fn row(ids: &[EntityId], id: EntityId) -> Option<usize> {
    ids.iter().position(|i| *i == id)
}

/// The night as last broadcast (none before the first broadcast arrives) and the night entity.
fn night_tick(ctx: &RunCtx<'_>) -> Result<Option<(EntityId, NightTick)>, String> {
    Ok(ctx.signals::<NightTick>()?.into_iter().last())
}

fn vec3(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}

fn yaw(angle: f32) -> Quat {
    Quat::from_xyzw(0.0, sinf(angle / 2.0), 0.0, cosf(angle / 2.0))
}

/// The lamplighter: walks by the `walk` axis, asks the body for the Light one-shot, kindles at
/// the `lit` event, pays for lamps lit, and says where they are.
pub(crate) fn walk(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let playing = night_tick(ctx)?.is_some_and(|(_, t)| t.playing());
    let inputs: Vec<InputActions> = ctx.read()?;
    let mut lighters: Vec<Lamplighter> = ctx.read()?;
    let mut transforms: Vec<Transform> = ctx.read()?;
    let mut motions: Vec<CharacterMotion> = ctx.read()?;
    for (id, _) in ctx.signals::<Lit>()? {
        if let Some(i) = row(&ids, id) {
            lighters[i].can = (lighters[i].can - rules::LIGHT_COST).max(0.0);
            lighters[i].lit += 1;
        }
    }
    let reached: Vec<EntityId> = ctx
        .signals::<Reached>()?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    for i in 0..ids.len() {
        let (velocity, amount) = if playing {
            rules::walk(inputs[i].axis("walk"))
        } else {
            ([0.0; 3], 0.0)
        };
        motions[i].velocity = vec3(velocity);
        if amount > 0.0 {
            transforms[i].rotation = yaw(atan2f(velocity[0], velocity[2]));
        }
        let light = playing && inputs[i].just_pressed("light") && rules::can_light(lighters[i].can);
        if let Some(body) = lighters[i].body {
            ctx.signal(
                body,
                &Stride {
                    walk_amount: amount,
                    light,
                },
            )?;
        }
        let at = transforms[i].translation;
        if playing && reached.contains(&ids[i]) && rules::can_light(lighters[i].can) {
            ctx.signal(ids[i], &Kindle { at })?;
        }
        ctx.signal(ids[i], &Where { at })?;
    }
    ctx.write(&lighters)?;
    ctx.write(&transforms)?;
    ctx.write(&motions)
}

/// A walker's body: takes its `Stride` into the graph, and tells the walker when `lit` fires.
pub(crate) fn body(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let bodies: Vec<Body> = ctx.read()?;
    let mut graphs: Vec<AnimGraphPlayer> = ctx.read()?;
    for (id, stride) in ctx.signals::<Stride>()? {
        if let Some(i) = row(&ids, id) {
            graphs[i]
                .floats
                .insert(WALK_AMOUNT.into(), stride.walk_amount);
            if stride.light {
                graphs[i].triggers.push(LIGHT_NOW.into());
            }
        }
    }
    for (id, event) in ctx.signals::<AnimEvent>()? {
        if let (Some(i), true) = (row(&ids, id), event.name == LIT_EVENT)
            && let Some(walker) = bodies[i].walker
        {
            ctx.signal(walker, &Reached)?;
        }
    }
    ctx.write(&graphs)
}

/// Lamps: track what stands in their light, light at a kindle in reach, burn oil, go out, and
/// scorch what is inside while lit.
pub(crate) fn lamps(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let dt = ctx.dt();
    let playing = night_tick(ctx)?.is_some_and(|(_, t)| t.playing());
    let mut lamps: Vec<Lamp> = ctx.read()?;
    let transforms: Vec<Transform> = ctx.read()?;
    let mut lights: Vec<PointLight> = ctx.read()?;
    for (id, contact) in ctx.signals::<ContactBegan>()? {
        if let Some(i) = row(&ids, id)
            && !lamps[i].inside.contains(&contact.other)
        {
            lamps[i].inside.push(contact.other);
        }
    }
    let mut gone: Vec<EntityId> = ctx
        .signals::<ContactEnded>()?
        .into_iter()
        .filter_map(|(id, c)| row(&ids, id).map(|_| c.other))
        .collect();
    gone.extend(
        ctx.signals::<Popped>()?
            .into_iter()
            .filter_map(|(_, p)| p.wisp),
    );
    gone.extend(
        ctx.signals::<SpiritLost>()?
            .into_iter()
            .filter_map(|(_, s)| s.wisp),
    );
    for lamp in &mut lamps {
        lamp.inside.retain(|e| !gone.contains(e));
    }
    for (kindler, kindle) in ctx.signals::<Kindle>()? {
        let spots: Vec<([f32; 3], bool)> = transforms
            .iter()
            .zip(&lamps)
            .map(|(t, l)| ([t.translation.x, kindle.at.y, t.translation.z], l.lit))
            .collect();
        if let Some(i) = rules::nearest_unlit([kindle.at.x, kindle.at.y, kindle.at.z], &spots) {
            lamps[i].lit = true;
            lamps[i].oil = rules::LAMP_SECONDS;
            ctx.signal(kindler, &Lit { lamp: Some(ids[i]) })?;
            if !lamps[i].lit_cue.is_empty() {
                let cue = PlayCue {
                    cue: lamps[i].lit_cue.clone(),
                    volume_db: 0.0,
                };
                ctx.signal(ids[i], &cue)?;
            }
        }
    }
    for i in 0..ids.len() {
        if lamps[i].lit && playing {
            lamps[i].oil = rules::burn_lamp(lamps[i].oil, dt);
            lamps[i].lit = lamps[i].oil > 0.0;
        }
        lights[i].intensity_lumens = if lamps[i].lit { lamps[i].lumens } else { 0.0 };
        if lamps[i].lit && playing {
            for other in lamps[i].inside.clone() {
                ctx.signal(other, &Scorch)?;
            }
        }
    }
    ctx.write(&lamps)?;
    ctx.write(&lights)
}

/// Spawns a rising number reading `text` from template `template` at `at`.
fn number(
    ctx: &mut RunCtx<'_>,
    template: &str,
    at: Vec3,
    text: &str,
    cue: Option<&str>,
) -> Result<(), String> {
    let spawned = ctx.spawn_template(template);
    ctx.insert(spawned, &Transform::at(at))?;
    ctx.insert(spawned, &UiText::centred(text))?;
    let rise = PropertyTween {
        component: "UiRoot".into(),
        field: "/space/World/offset/1".into(),
        from: Some(vec![NUMBER_HEIGHT]),
        to: vec![NUMBER_HEIGHT + NUMBER_RISE],
        duration: NUMBER_SECONDS,
        transition: Transition::Sine,
        ease: Ease::Out,
        ..PropertyTween::default()
    };
    ctx.insert(spawned, &Tweens { tweens: vec![rise] })?;
    ctx.insert(
        spawned,
        &Number {
            ticks_left: NUMBER_TICKS,
        },
    )?;
    if let Some(cue) = cue.filter(|c| !c.is_empty()) {
        ctx.signal(
            spawned,
            &PlayCue {
                cue: cue.to_owned(),
                volume_db: 0.0,
            },
        )?;
    }
    Ok(())
}

/// Wisps: burn in lamplight (with numbers and a sizzle), pop at no health, and cost spirit when
/// they reach the square.
pub(crate) fn wisps(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let dt = ctx.dt();
    let Some((night_id, tick)) = night_tick(ctx)? else {
        return Ok(());
    };
    let mut wisps: Vec<Wisp> = ctx.read()?;
    let transforms: Vec<Transform> = ctx.read()?;
    let mut sounds: Vec<AudioEmitter> = ctx.read()?;
    let mut lights = vec![0u32; ids.len()];
    for (id, _) in ctx.signals::<Scorch>()? {
        if let Some(i) = row(&ids, id) {
            lights[i] += 1;
        }
    }
    let arrived: Vec<EntityId> = ctx
        .signals::<NavArrived>()?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    for i in 0..ids.len() {
        let at = transforms[i].translation;
        if arrived.contains(&ids[i]) {
            ctx.despawn(ids[i]);
            ctx.signal(night_id, &SpiritLost { wisp: Some(ids[i]) })?;
            continue;
        }
        let in_light = tick.playing() && lights[i] > 0;
        sounds[i].playing = in_light;
        if !in_light {
            continue;
        }
        let (health, popped) = rules::scorch(wisps[i].health, lights[i], dt);
        wisps[i].health = health;
        wisps[i].burn_ticks += 1;
        if popped {
            ctx.despawn(ids[i]);
            ctx.signal(
                night_id,
                &Popped {
                    wisp: Some(ids[i]),
                    at,
                },
            )?;
            let shown = rules::pop_number(wisps[i].burn_ticks, lights[i]);
            if let Some(n) = shown {
                number(
                    ctx,
                    "burn_number",
                    at,
                    &n.to_string(),
                    Some(&wisps[i].pop_cue),
                )?;
            }
        } else if let Some(n) = rules::burn_number(wisps[i].burn_ticks, lights[i]) {
            number(ctx, "burn_number", at, &n.to_string(), None)?;
        }
    }
    ctx.write(&wisps)?;
    ctx.write(&sounds)
}

/// Floating numbers: count down and go.
pub(crate) fn numbers(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let mut numbers: Vec<Number> = ctx.read()?;
    for (i, n) in numbers.iter_mut().enumerate() {
        n.ticks_left = n.ticks_left.saturating_sub(1);
        if n.ticks_left == 0 {
            ctx.despawn(ids[i]);
        }
    }
    ctx.write(&numbers)
}

/// The night: runs the clock while played, counts lamps, pops and spirit, ends at dawn or with
/// no spirit, and tells every module about itself.
pub(crate) fn night(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let dt = ctx.dt();
    let mut nights: Vec<Night> = ctx.read()?;
    let lost = ctx.signals::<SpiritLost>()?.len() as u32;
    let popped = ctx.signals::<Popped>()?.len() as u32;
    let lit = ctx.signals::<Lit>()?.len() as u32;
    for i in 0..ids.len() {
        let n = &mut nights[i];
        if n.phase == night::PLAYING {
            n.clock += dt;
            n.lamps_lit += lit;
            n.wisps_burned += popped;
            for _ in 0..lost.min(n.spirit) {
                n.spirit -= 1;
                number(
                    ctx,
                    "spirit_number",
                    n.square + Vec3::Y * (SPIRIT_HEIGHT - NUMBER_HEIGHT),
                    "-1",
                    None,
                )?;
            }
            match rules::outcome(n.clock, n.length, n.spirit) {
                Some(Outcome::Won) => {
                    n.phase = night::WON.into();
                    for cue in n.dawn_cues.clone() {
                        ctx.signal(
                            ids[i],
                            &PlayCue {
                                cue,
                                volume_db: 0.0,
                            },
                        )?;
                    }
                }
                Some(Outcome::Lost) => n.phase = night::LOST.into(),
                None => {}
            }
        }
        let tick = NightTick {
            phase: n.phase.clone(),
            clock: n.clock,
            length: n.length,
            spirit: n.spirit,
            density: n.density,
        };
        ctx.signal(ids[i], &tick)?;
    }
    ctx.write(&nights)
}

/// The chase camera: sits at `offset` from where the lamplighter was, looking down at them.
pub(crate) fn chase(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let Some((_, at)) = ctx.signals::<Where>()?.into_iter().last() else {
        return Ok(());
    };
    let chases: Vec<Chase> = ctx.read()?;
    let mut transforms: Vec<Transform> = ctx.read()?;
    for (chase, t) in chases.iter().zip(&mut transforms) {
        t.translation = at.at + chase.offset;
        let down = atan2f(chase.offset.y, chase.offset.z);
        t.rotation = Quat::from_xyzw(sinf(-down / 2.0), 0.0, 0.0, cosf(-down / 2.0));
    }
    ctx.write(&transforms)
}
