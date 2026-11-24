//! Lamplight's fog module, `fog` (M2 phase 2.10c.1), loaded as a native library so it can be
//! hot-reloaded mid-night: while the night is played it sends waves of wisps in from the harbour
//! and steers each toward the square.

mod people;
pub mod townsfolk;
mod waves;

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_script_guest::{
    EntityId, Module, ModuleDesc, Phase, RunCtx, describe_type, export_module, processor,
};
use lamplight_types::engine::{NavAgent, NavArrived, SimLod, Transform};
use lamplight_types::night::{NewNight, NightTick};
use serde::{Deserialize, Serialize};

/// The fog's director: when the next wave comes, how big waves are, and where they come from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "fog.Fog", default)]
pub struct Fog {
    /// Seconds between waves.
    #[reflect(range(min = 1.0, max = 600.0))]
    pub every: f32,
    /// Seconds from a night's start to its first wave.
    pub first: f32,
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
            first: 5.0,
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

/// A townsperson: strolls between home and the market, and flees home from wisps.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "fog.Townsperson", default)]
pub struct Townsperson {
    /// Their front door.
    pub home: Vec3,
    /// Their market stall.
    pub market: Vec3,
    /// Walking home (else to market).
    pub homeward: bool,
    /// Frightened: fleeing, or sheltering until `calm_in` runs out.
    pub scared: bool,
    /// Seconds until a frightened townsperson strolls again.
    pub calm_in: f32,
    /// Where they stood at their last update.
    pub last: Vec3,
    /// Updates they have had (simulation LOD makes far ones update less often).
    pub updates: u32,
    /// Their animated body.
    pub body: Option<EntityId>,
}

impl Default for Townsperson {
    fn default() -> Self {
        Self {
            home: Vec3::ZERO,
            market: Vec3::ZERO,
            homeward: false,
            scared: false,
            calm_in: 0.0,
            last: Vec3::ZERO,
            updates: 0,
            body: None,
        }
    }
}

/// Signal: a wisp is at `at` (on the wisp, every tick).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "fog.WispAt", default)]
pub struct WispAt {
    /// Where.
    pub at: Vec3,
}

/// The module.
pub struct FogModule;

impl Module for FogModule {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "fog".into(),
            version: 1,
            types: Vec::new(),
            components: vec![
                describe_type::<Fog>(),
                describe_type::<Drifter>(),
                describe_type::<Townsperson>(),
            ],
            signals: vec![describe_type::<WispAt>()],
            traits: Vec::new(),
            processors: vec![
                processor("fog.waves", Phase::PrePhysics)
                    .writes::<Fog>()
                    .receives::<NightTick>()
                    .receives::<NewNight>()
                    .build(),
                processor("fog.steer", Phase::PrePhysics)
                    .reads::<Drifter>()
                    .writes::<NavAgent>()
                    .build(),
                processor("fog.watch", Phase::PostPhysics)
                    .with::<Drifter>()
                    .reads::<Transform>()
                    .build(),
                processor("fog.townsfolk", Phase::PrePhysics)
                    .writes::<Townsperson>()
                    .writes::<NavAgent>()
                    .writes::<Transform>()
                    .reads::<SimLod>()
                    .receives::<NavArrived>()
                    .receives::<WispAt>()
                    .after("crowd.move")
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "fog.waves" => waves::waves(ctx),
            "fog.steer" => waves::steer(ctx),
            "fog.watch" => people::watch(ctx),
            "fog.townsfolk" => people::townsfolk(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

export_module!(FogModule);
