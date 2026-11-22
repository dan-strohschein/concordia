//! Navigation agents (M2 phase 2.5b.1): what gameplay asks for (`NavAgent`), what the engine
//! keeps while following (`NavAgentState`, hashed), and the signals it raises.

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::NavError;

/// Walks its entity's `CharacterController` along navmesh paths to `target` (Godot
/// `NavigationAgent3D` semantics). Gameplay writes it; the engine only reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct NavAgent {
    /// Where to go (none: stand still).
    pub target: Option<Vec3>,
    /// Walking speed in meters per second.
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub max_speed: f32,
    /// Agent radius in meters (should match the controller and fit the navmesh).
    #[reflect(range(min = 0.0, max = 50.0))]
    pub radius: f32,
    /// Agent height in meters (should match the controller).
    #[reflect(range(min = 0.0, max = 100.0))]
    pub height: f32,
    /// A path corner counts as reached within this sideways distance, in meters.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub path_desired_distance: f32,
    /// The target counts as reached within this sideways distance, in meters.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub target_desired_distance: f32,
    /// Farther than this sideways from its path, the agent plans again, in meters.
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub path_max_distance: f32,
    /// The volume whose navmesh to use (none: the first by id whose box holds the agent).
    pub volume: Option<EntityId>,
}

impl Default for NavAgent {
    /// Idle, sized like the default character controller.
    fn default() -> Self {
        Self {
            target: None,
            max_speed: 4.0,
            radius: 0.3,
            height: 1.8,
            path_desired_distance: 0.5,
            target_desired_distance: 0.5,
            path_max_distance: 2.0,
            volume: None,
        }
    }
}

impl NavAgent {
    /// Checks every field on `entity`'s agent, naming the first bad one.
    pub fn validate(&self, entity: EntityId) -> Result<(), NavError> {
        self.check()
            .map_err(|message| NavError::Invalid { entity, message })
    }

    fn check(&self) -> Result<(), String> {
        let positive = |name: &str, x: f32| match x.is_finite() && x > 0.0 {
            true => Ok(()),
            false => Err(format!(
                "NavAgent.{name} must be a positive number, got {x}"
            )),
        };
        positive("max_speed", self.max_speed)?;
        positive("radius", self.radius)?;
        positive("height", self.height)?;
        positive("path_desired_distance", self.path_desired_distance)?;
        positive("target_desired_distance", self.target_desired_distance)?;
        positive("path_max_distance", self.path_max_distance)?;
        match self.target {
            Some(t) if !t.is_finite() => Err(format!("NavAgent.target must be finite, got {t}")),
            _ => Ok(()),
        }
    }

    /// `a` is within `distance` of `b` sideways and within half the agent's height vertically.
    pub fn near(&self, a: Vec3, b: Vec3, distance: f32) -> bool {
        sideways(a, b) <= distance && (a.y - b.y).abs() <= self.height * 0.5
    }
}

/// The distance from `a` to `b` on the ground plane.
pub fn sideways(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
}

/// Where an agent is in following its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum NavStatus {
    /// No target.
    #[default]
    Idle,
    /// Needs a plan and is waiting for the per-tick budget.
    Waiting,
    /// Following its path.
    Moving,
    /// Reached its target (or the closest reachable point).
    Arrived,
    /// No path for this target; retried when the target moves.
    Failed,
}

/// The engine's following state for a `NavAgent` (hashed; written only by `nav.plan` and
/// `nav.follow`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct NavAgentState {
    /// Where it is in following.
    pub status: NavStatus,
    /// The target its path (or failure) is for.
    pub planned_for: Option<Vec3>,
    /// The path's corners, from the start to the end (on the navmesh surface).
    pub waypoints: Vec<Vec3>,
    /// The index of the corner being walked to.
    pub next: u32,
    /// The path ends at the closest reachable point, not the target.
    pub partial: bool,
    /// The volume whose navmesh the path is on.
    pub volume: Option<EntityId>,
    /// The tick it started waiting for a plan (for the budget's order).
    pub waiting_since: u64,
    /// A crowd agent's polygon corridor: Detour references from its polygon to the end (empty
    /// for other agents).
    pub corridor: Vec<u64>,
}

/// Raised on an agent when it reaches its target, or the closest reachable point; delivered next
/// tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation")]
#[serde(deny_unknown_fields)]
pub struct NavArrived {
    /// The target it was asked to reach.
    pub target: Vec3,
    /// Where its feet were when it arrived.
    pub position: Vec3,
    /// The target was unreachable; it arrived at the closest reachable point.
    pub partial: bool,
}

/// Why an agent has no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum NavFailure {
    /// No volume holds the agent, or its navmesh wasn't built.
    NoNavmesh,
    /// The agent is too far from the navmesh.
    StartOffNavmesh,
    /// The target is too far from the navmesh.
    TargetOffNavmesh,
    /// The agent's settings are invalid, or it has no `CharacterController`.
    InvalidAgent,
}

/// Raised on an agent, once per target, when no path can be planned; delivered next tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation")]
#[serde(deny_unknown_fields)]
pub struct NavPathFailed {
    /// The target it was asked to reach.
    pub target: Vec3,
    /// Why.
    pub reason: NavFailure,
    /// A sentence saying what was wrong.
    pub detail: String,
}
