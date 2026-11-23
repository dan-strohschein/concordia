//! Lamplight's fog module, `fog` (M2 phase 2.10c.1), loaded as a native library so it can be
//! hot-reloaded mid-night: while the night is played it sends waves of wisps in from the harbour
//! and steers each toward the square.

mod waves;

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_script_guest::{
    Module, ModuleDesc, Phase, RunCtx, describe_type, export_module, processor,
};
use lamplight_types::engine::NavAgent;
use lamplight_types::night::NightTick;
use serde::{Deserialize, Serialize};

/// The fog's director: when the next wave comes, how big waves are, and where they come from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "fog.Fog", default)]
pub struct Fog {
    /// Seconds between waves.
    #[reflect(range(min = 1.0, max = 600.0))]
    pub every: f32,
    /// Seconds until the next wave.
    pub next: f32,
    /// Wisps in a wave at density 1.
    pub per_wave: u32,
    /// Waves sent this night.
    pub waves: u32,
    /// The harbour edge's centre, where wisps rise.
    pub origin: Vec3,
    /// Half the harbour edge's size (x and z).
    pub half_extents: Vec3,
    /// Where wisps head.
    pub square: Vec3,
    /// Wisps aim at a ring around the square from this far out (the fountain fills the middle).
    pub inner: f32,
    /// To this far out.
    pub outer: f32,
}

impl Default for Fog {
    fn default() -> Self {
        Self {
            every: 20.0,
            next: 5.0,
            per_wave: 6,
            waves: 0,
            origin: Vec3::new(0.0, 0.0, 30.0),
            half_extents: Vec3::new(16.0, 0.0, 2.0),
            square: Vec3::ZERO,
            inner: 4.0,
            outer: 6.0,
        }
    }
}

/// Where a wisp drifts to (the fog sets its `NavAgent` target from it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "fog.Drifter", default)]
pub struct Drifter {
    /// The point it heads for.
    pub target: Vec3,
}

/// The module.
pub struct FogModule;

impl Module for FogModule {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "fog".into(),
            version: 1,
            types: Vec::new(),
            components: vec![describe_type::<Fog>(), describe_type::<Drifter>()],
            signals: Vec::new(),
            traits: Vec::new(),
            processors: vec![
                processor("fog.waves", Phase::PrePhysics)
                    .writes::<Fog>()
                    .receives::<NightTick>()
                    .build(),
                processor("fog.steer", Phase::PrePhysics)
                    .reads::<Drifter>()
                    .writes::<NavAgent>()
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "fog.waves" => waves::waves(ctx),
            "fog.steer" => waves::steer(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

export_module!(FogModule);
