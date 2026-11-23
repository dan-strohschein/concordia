//! The input guest module (`inputg`, M2 phase 2.10a): a guest that reads player input through
//! the engine's `InputActions` component, mirrored field for field (ADR-0062: wire layouts come
//! from shapes, and names resolve against the engine's registry). `inputg.walk` sets each
//! walker's `CharacterMotion` to the `camera_pan` axis (x, 0, −y) times its speed, and counts
//! `spawn_box` presses.

use std::collections::BTreeMap;

use concordia_core::math::{Vec2, Vec3};
use concordia_reflect::Reflect;
use concordia_script_guest::{
    Module, ModuleDesc, Phase, RunCtx, describe_type, export_module, processor,
};
use serde::{Deserialize, Serialize};

/// A walker's speed and how many presses it has seen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "inputg.Walker", default)]
pub struct Walker {
    /// Meters per second at full stick.
    pub speed: f32,
    /// `spawn_box` presses seen.
    pub presses: u32,
}

/// The engine's `ActionValue`, mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ActionValue")]
pub enum ActionValue {
    /// Button state.
    Button(bool),
    /// One axis.
    Axis1d(f32),
    /// Two axes.
    Axis2d(Vec2),
    /// A ray.
    Ray {
        /// Start point.
        origin: Vec3,
        /// Unit direction.
        direction: Vec3,
    },
}

/// The engine's `ActionState`, mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ActionState")]
pub struct ActionState {
    /// Held at the end of the tick.
    pub pressed: bool,
    /// Pressed during the tick.
    pub just_pressed: bool,
    /// Released during the tick.
    pub just_released: bool,
    /// The value.
    pub value: ActionValue,
}

/// The engine's `InputActions`, mirrored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "InputActions", default)]
pub struct InputActions {
    /// Watched action names.
    pub watch: Vec<String>,
    /// Their states this tick.
    pub states: BTreeMap<String, ActionState>,
}

/// The engine's `CharacterMotion`, mirrored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "CharacterMotion", default)]
pub struct CharacterMotion {
    /// Meters per second.
    pub velocity: Vec3,
}

/// The module.
pub struct InputGuest;

impl Module for InputGuest {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "inputg".into(),
            version: 1,
            types: Vec::new(),
            components: vec![describe_type::<Walker>()],
            signals: Vec::new(),
            traits: Vec::new(),
            processors: vec![
                processor("inputg.walk", Phase::PrePhysics)
                    .reads::<InputActions>()
                    .writes::<Walker>()
                    .writes::<CharacterMotion>()
                    .after("input.actions")
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "inputg.walk" => walk(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

fn walk(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let inputs: Vec<InputActions> = ctx.read()?;
    let mut walkers: Vec<Walker> = ctx.read()?;
    let mut motions: Vec<CharacterMotion> = ctx.read()?;
    for ((input, walker), motion) in inputs.iter().zip(&mut walkers).zip(&mut motions) {
        let axis = match input.states.get("camera_pan").map(|s| s.value) {
            Some(ActionValue::Axis2d(v)) => v,
            _ => Vec2::ZERO,
        };
        motion.velocity = Vec3::new(axis.x, 0.0, -axis.y) * walker.speed;
        if input
            .states
            .get("spawn_box")
            .is_some_and(|s| s.just_pressed)
        {
            walker.presses += 1;
        }
    }
    ctx.write(&walkers)?;
    ctx.write(&motions)
}

export_module!(InputGuest);
