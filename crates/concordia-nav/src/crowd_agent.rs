//! Crowd agents (M2 phase 2.5c): a `NavAgent` the crowd steers (`concordia-terra-crowd`), with
//! DetourCrowd's agent parameters (zlib source, `dtCrowdAgentParams`) and the defaults its sample
//! uses for a 0.3 m agent.

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::NavError;

/// The crowd steers this entity's `NavAgent`: neighbours, separation, avoidance and collision
/// (Godot's `avoidance_enabled`). Without a `CharacterController` it moves on the navmesh surface;
/// with one, the crowd's velocity goes to `CharacterMotion`. Gameplay writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct CrowdAgent {
    /// Most change of velocity per second, in meters per second squared.
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub max_acceleration: f32,
    /// Neighbours and walls within this sideways distance are considered, in meters.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub collision_query_range: f32,
    /// Farthest a visibility shortcut looks ahead, in meters.
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub path_optimization_range: f32,
    /// Steer early toward the corner after next.
    pub anticipate_turns: bool,
    /// Shortcut the corridor toward corners in plain sight.
    pub optimize_visibility: bool,
    /// Push away from neighbours while steering.
    pub separation: bool,
    /// How strongly separation pushes.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub separation_weight: f32,
    /// Sampled velocity-obstacle avoidance (none: off).
    pub avoidance: Option<CrowdAvoidance>,
}

impl Default for CrowdAgent {
    fn default() -> Self {
        Self {
            max_acceleration: 8.0,
            collision_query_range: 3.6,
            path_optimization_range: 9.0,
            anticipate_turns: true,
            optimize_visibility: true,
            separation: false,
            separation_weight: 2.0,
            avoidance: Some(CrowdAvoidance::default()),
        }
    }
}

/// DetourCrowd's adaptive velocity sampling (`dtObstacleAvoidanceParams`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct CrowdAvoidance {
    /// How much the first sample ring leans toward the desired velocity (0..=1).
    #[reflect(range(min = 0.0, max = 1.0))]
    pub velocity_bias: f32,
    /// Penalty for straying from the desired velocity.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub weight_desired: f32,
    /// Penalty for changing the current velocity.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub weight_current: f32,
    /// Penalty for passing obstacles on the wrong side.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub weight_side: f32,
    /// Penalty for an early time of impact.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub weight_toi: f32,
    /// Seconds ahead that impacts count.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub horizon: f32,
    /// Samples per ring (1..=32).
    pub divs: u8,
    /// Rings (1..=4).
    pub rings: u8,
    /// Refinement passes, each halving the ring radius (1..=8).
    pub depth: u8,
}

impl Default for CrowdAvoidance {
    /// `dtCrowd::init`'s parameters.
    fn default() -> Self {
        Self {
            velocity_bias: 0.4,
            weight_desired: 2.0,
            weight_current: 0.75,
            weight_side: 0.75,
            weight_toi: 2.5,
            horizon: 2.5,
            divs: 7,
            rings: 2,
            depth: 5,
        }
    }
}

impl CrowdAgent {
    /// Checks every field on `entity`'s crowd agent, naming the first bad one.
    pub fn validate(&self, entity: EntityId) -> Result<(), NavError> {
        self.check()
            .map_err(|message| NavError::Invalid { entity, message })
    }

    fn check(&self) -> Result<(), String> {
        positive("CrowdAgent.max_acceleration", self.max_acceleration)?;
        positive(
            "CrowdAgent.collision_query_range",
            self.collision_query_range,
        )?;
        positive(
            "CrowdAgent.path_optimization_range",
            self.path_optimization_range,
        )?;
        at_least_zero("CrowdAgent.separation_weight", self.separation_weight)?;
        self.avoidance
            .as_ref()
            .map_or(Ok(()), CrowdAvoidance::check)
    }
}

impl CrowdAvoidance {
    fn check(&self) -> Result<(), String> {
        let bias = self.velocity_bias;
        if !(bias.is_finite() && (0.0..=1.0).contains(&bias)) {
            return Err(format!(
                "CrowdAvoidance.velocity_bias must be in 0..=1, got {bias}"
            ));
        }
        at_least_zero("CrowdAvoidance.weight_desired", self.weight_desired)?;
        at_least_zero("CrowdAvoidance.weight_current", self.weight_current)?;
        at_least_zero("CrowdAvoidance.weight_side", self.weight_side)?;
        at_least_zero("CrowdAvoidance.weight_toi", self.weight_toi)?;
        positive("CrowdAvoidance.horizon", self.horizon)?;
        within("CrowdAvoidance.divs", self.divs, 32)?;
        within("CrowdAvoidance.rings", self.rings, 4)?;
        within("CrowdAvoidance.depth", self.depth, 8)
    }
}

fn positive(name: &str, x: f32) -> Result<(), String> {
    match x.is_finite() && x > 0.0 {
        true => Ok(()),
        false => Err(format!("{name} must be a positive number, got {x}")),
    }
}

fn at_least_zero(name: &str, x: f32) -> Result<(), String> {
    match x.is_finite() && x >= 0.0 {
        true => Ok(()),
        false => Err(format!("{name} must be a finite number ≥ 0, got {x}")),
    }
}

fn within(name: &str, n: u8, max: u8) -> Result<(), String> {
    match (1..=max).contains(&n) {
        true => Ok(()),
        false => Err(format!("{name} must be in 1..={max}, got {n}")),
    }
}
