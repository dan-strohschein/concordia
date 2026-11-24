//! The fixture's gameplay module, `player`: each frame the player walks where the `move` action
//! points (W is forward, +Z), at its controller's speed.

use std::collections::BTreeMap;

use concordia_core::math::{Quat, Vec2, Vec3};
use concordia_reflect::Reflect;
use concordia_script_guest::{
    Module, ModuleDesc, Phase, RunCtx, describe_type, export_module, processor,
};
use serde::{Deserialize, Serialize};

/// How fast the player walks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "player.Controller", default)]
pub struct Controller {
    /// Meters per second at full stick.
    pub speed: f32,
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

/// The engine's `Transform`, mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Transform")]
pub struct Transform {
    /// Position.
    pub translation: Vec3,
    /// Orientation.
    pub rotation: Quat,
    /// Scale.
    pub scale: Vec3,
}

/// How far the player moves this frame along one axis, at `speed` with the stick at `axis`.
pub fn step(axis: f32, speed: f32, dt: f32) -> f32 {
    axis * speed * dt * dt
}

/// The module.
pub struct Player;

impl Module for Player {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "player".into(),
            version: 1,
            types: Vec::new(),
            components: vec![describe_type::<Controller>()],
            signals: Vec::new(),
            traits: Vec::new(),
            processors: vec![
                processor("player.walk", Phase::PrePhysics)
                    .reads::<InputActions>()
                    .reads::<Controller>()
                    .writes::<Transform>()
                    .after("input.actions")
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "player.walk" => walk(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

fn walk(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let dt = ctx.dt();
    let inputs: Vec<InputActions> = ctx.read()?;
    let controllers: Vec<Controller> = ctx.read()?;
    let mut transforms: Vec<Transform> = ctx.read()?;
    for ((input, controller), transform) in inputs.iter().zip(&controllers).zip(&mut transforms) {
        let axis = match input.states.get("move").map(|s| s.value) {
            Some(ActionValue::Axis2d(v)) => v,
            _ => Vec2::ZERO,
        };
        transform.translation.x += step(axis.x, controller.speed, dt);
        transform.translation.z += step(axis.y, controller.speed, dt);
    }
    ctx.write(&transforms)
}

export_module!(Player);

