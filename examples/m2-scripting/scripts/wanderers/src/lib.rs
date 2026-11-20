//! Sample gameplay module `wanderers` (M2 phase 2.6d): wanderers drift with a heading that turns
//! at random, spend energy as they move, signal when they are tired, rest until recovered, and
//! start wandering again. It exercises the per-processor random stream, libm trigonometry,
//! column writes, commands and signals, so every backend must agree on it bit for bit.
//!
//! Build it as WASM (`cargo build --target wasm32-wasip2`), as a native library
//! (`--features dylib`), or link it statically.

use concordia_reflect::Reflect;
use concordia_script_guest::math::{cosf, sinf};
use concordia_script_guest::{
    Module, ModuleDesc, Phase, RunCtx, TraitDesc, describe_type, export_module, processor,
};
use serde::{Deserialize, Serialize};

/// Where a wanderer is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "wanderers.Position", default)]
pub struct Position {
    /// X (meters).
    pub x: f32,
    /// Y (meters).
    pub y: f32,
}

/// How a wanderer moves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "wanderers.Wander", default)]
pub struct Wander {
    /// Direction (radians).
    pub heading: f32,
    /// Meters per second.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub speed: f32,
    /// Largest turn per second (radians).
    #[reflect(range(min = 0.0, max = 10.0))]
    pub turn: f32,
}

impl Default for Wander {
    fn default() -> Self {
        Self {
            heading: 0.0,
            speed: 1.5,
            turn: 2.0,
        }
    }
}

/// Energy: spent while wandering, regained while resting.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "wanderers.Energy", default)]
pub struct Energy {
    /// 0 to 1.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub value: f32,
}

impl Default for Energy {
    fn default() -> Self {
        Self { value: 1.0 }
    }
}

/// Present while a wanderer rests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "wanderers.Resting", default)]
pub struct Resting;

/// Raised when a wanderer runs out of energy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "wanderers.Tired", default)]
pub struct Tired {
    /// Where it stopped.
    pub at: Position,
}

/// Energy spent per second of wandering.
const SPEND: f32 = 0.25;
/// Energy regained per second of rest.
const REGAIN: f32 = 0.5;

/// The module.
pub struct Wanderers;

impl Module for Wanderers {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "wanderers".into(),
            version: 1,
            types: Vec::new(),
            components: vec![
                describe_type::<Position>(),
                describe_type::<Wander>(),
                describe_type::<Energy>(),
                describe_type::<Resting>(),
            ],
            signals: vec![describe_type::<Tired>()],
            traits: vec![TraitDesc {
                name: "wanderers.Wanderer".into(),
                docs: "Wanders at random, tires and rests.".into(),
                components: vec![
                    "wanderers.Position".into(),
                    "wanderers.Wander".into(),
                    "wanderers.Energy".into(),
                ],
                requires: vec!["wanderers.walk".into(), "wanderers.rest".into()],
            }],
            processors: vec![
                processor("wanderers.walk", Phase::PostPhysics)
                    .writes::<Position>()
                    .writes::<Wander>()
                    .writes::<Energy>()
                    .without::<Resting>()
                    .build(),
                processor("wanderers.tired", Phase::FrameEnd)
                    .receives::<Tired>()
                    .build(),
                processor("wanderers.rest", Phase::PostPhysics)
                    .writes::<Energy>()
                    .with::<Resting>()
                    .after("wanderers.walk")
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "wanderers.walk" => walk(ctx),
            "wanderers.tired" => tired(ctx),
            "wanderers.rest" => rest(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

fn walk(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let dt = ctx.dt();
    let mut positions: Vec<Position> = ctx.read()?;
    let mut wanders: Vec<Wander> = ctx.read()?;
    let mut energies: Vec<Energy> = ctx.read()?;
    for i in 0..positions.len() {
        let w = &mut wanders[i];
        w.heading += (ctx.rng().next_f32() * 2.0 - 1.0) * w.turn * dt;
        positions[i].x += cosf(w.heading) * w.speed * dt;
        positions[i].y += sinf(w.heading) * w.speed * dt;
        energies[i].value = (energies[i].value - SPEND * dt).max(0.0);
        if energies[i].value == 0.0 {
            let id = ctx.ids()[i];
            ctx.signal(id, &Tired { at: positions[i] })?;
        }
    }
    ctx.write(&positions)?;
    ctx.write(&wanders)?;
    ctx.write(&energies)
}

fn tired(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    for (id, tired) in ctx.signals::<Tired>()? {
        ctx.log(&format!("tired at ({:.2}, {:.2})", tired.at.x, tired.at.y));
        ctx.insert(id, &Resting)?;
    }
    Ok(())
}

fn rest(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let dt = ctx.dt();
    let mut energies: Vec<Energy> = ctx.read()?;
    for (i, e) in energies.iter_mut().enumerate() {
        e.value = (e.value + REGAIN * dt).min(1.0);
        if e.value >= 1.0 {
            let id = ctx.ids()[i];
            ctx.remove::<Resting>(id);
        }
    }
    ctx.write(&energies)
}

export_module!(Wanderers);
