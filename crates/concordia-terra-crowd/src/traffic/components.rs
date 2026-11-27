//! Traffic's components and signals (M2 phase 2.5d, ADR-0079): lanes and junctions are scene
//! entities (N6); vehicles follow them with the Intelligent Driver Model.

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// A lane: a polyline vehicles drive along, in its entity's space (translation and rotation, not
/// scale), from its first point to its last. The level designer writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Traffic", default)]
#[serde(default, deny_unknown_fields)]
pub struct Lane {
    /// The centre line, at least two points, in driving order.
    pub points: Vec<Vec3>,
    /// The fastest a vehicle may drive along it, in meters per second.
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub speed_limit: f32,
    /// How wide it is, in meters (people within half of it are in the way).
    #[reflect(range(min = 0.0, max = 100.0))]
    pub width: f32,
    /// The lanes it leads to.
    pub next: Vec<EntityId>,
    /// Higher goes first where junction lanes conflict.
    pub priority: u8,
}

impl Default for Lane {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            speed_limit: 13.9,
            width: 3.5,
            next: Vec::new(),
            priority: 0,
        }
    }
}

/// A junction: the lanes inside it, of which conflicting ones (crossing, or merging into the same
/// lane) are never driven at once.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Traffic", default)]
#[serde(default, deny_unknown_fields)]
pub struct Junction {
    /// Its connector lanes.
    pub lanes: Vec<EntityId>,
}

/// A vehicle: the Intelligent Driver Model's parameters (Treiber, Hennecke and Helbing 2000) and
/// where it is going. Gameplay writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Traffic", default)]
#[serde(default, deny_unknown_fields)]
pub struct Vehicle {
    /// Its own top speed, in meters per second (a lane's limit may be lower).
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub max_speed: f32,
    /// How hard it accelerates, in meters per second squared.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub max_acceleration: f32,
    /// How hard it likes to brake, in meters per second squared.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub comfortable_braking: f32,
    /// Seconds of gap it keeps to the vehicle ahead.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub time_headway: f32,
    /// The gap it keeps when stopped, in meters.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub min_gap: f32,
    /// Bumper to bumper, in meters.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub length: f32,
    /// The lane to drive to (none: wander).
    pub destination: Option<EntityId>,
}

impl Default for Vehicle {
    /// The IDM paper's urban values.
    fn default() -> Self {
        Self {
            max_speed: 15.0,
            max_acceleration: 1.0,
            comfortable_braking: 1.5,
            time_headway: 1.5,
            min_gap: 2.0,
            length: 4.5,
            destination: None,
        }
    }
}

impl Vehicle {
    /// Checks every field, naming the first bad one.
    pub fn check(&self) -> Result<(), String> {
        positive("Vehicle.max_speed", self.max_speed)?;
        positive("Vehicle.max_acceleration", self.max_acceleration)?;
        positive("Vehicle.comfortable_braking", self.comfortable_braking)?;
        at_least_zero("Vehicle.time_headway", self.time_headway)?;
        at_least_zero("Vehicle.min_gap", self.min_gap)?;
        positive("Vehicle.length", self.length)
    }
}

impl Lane {
    /// Checks the fields that don't need the rest of the graph, naming the first bad one.
    pub fn check(&self) -> Result<(), String> {
        if self.points.len() < 2 {
            return Err(format!(
                "Lane.points needs at least 2 points, got {}",
                self.points.len()
            ));
        }
        if let Some(p) = self.points.iter().find(|p| !p.is_finite()) {
            return Err(format!("Lane.points has a non-finite point {p}"));
        }
        positive("Lane.speed_limit", self.speed_limit)?;
        positive("Lane.width", self.width)
    }
}

/// What a vehicle is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum VehicleStatus {
    /// Not on a lane yet.
    #[default]
    Idle,
    /// Driving.
    Driving,
    /// Stopped or braking at a junction it hasn't been let into.
    Waiting,
    /// Stopped at the end of its destination lane.
    Arrived,
    /// Its lane is bad, or its destination unreachable.
    Failed,
}

/// Where a vehicle is on the lane graph (hashed; written only by `traffic.*`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Traffic", default)]
#[serde(default, deny_unknown_fields)]
pub struct VehicleState {
    /// The lane its centre is on.
    pub lane: Option<EntityId>,
    /// Meters along that lane, to its centre.
    pub distance: f32,
    /// Its speed.
    pub speed: f32,
    /// Lanes still to drive, toward its destination.
    pub route: Vec<EntityId>,
    /// The destination its route is for.
    pub routed_for: Option<EntityId>,
    /// What it is doing.
    pub status: VehicleStatus,
    /// The tick it started waiting at its junction.
    pub waiting_since: u64,
}

/// One vehicle let into one connector lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Traffic")]
#[serde(deny_unknown_fields)]
pub struct Grant {
    /// The vehicle.
    pub vehicle: EntityId,
    /// The connector it may drive.
    pub lane: EntityId,
}

/// Who a junction has let in (hashed; written only by `traffic.junctions`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Traffic", default)]
#[serde(default, deny_unknown_fields)]
pub struct JunctionState {
    /// Grants still held, by vehicle id.
    pub granted: Vec<Grant>,
}

/// What is wrong with a lane or junction, put on it in the play world at play start (so
/// `query.run` finds it); traffic ignores the entity's bad parts.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Traffic", default)]
#[serde(default, deny_unknown_fields)]
pub struct TrafficProblems {
    /// Each problem, naming the component and field.
    pub messages: Vec<String>,
}

/// Raised on a vehicle when it stops at the end of its destination lane; delivered next tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "Traffic")]
#[serde(deny_unknown_fields)]
pub struct VehicleArrived {
    /// The lane it was sent to.
    pub destination: EntityId,
    /// Where its centre stopped.
    pub position: Vec3,
}

/// Raised on a vehicle once per destination it can't reach; delivered next tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "Traffic")]
#[serde(deny_unknown_fields)]
pub struct VehicleRouteFailed {
    /// The lane it was sent to.
    pub destination: EntityId,
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
