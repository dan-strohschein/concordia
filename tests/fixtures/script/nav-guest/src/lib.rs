//! The nav guest module (`navg`, M2 phase 2.5b.1): a WASM guest that drives an engine component
//! and hears engine signals by name. `navg.go` writes each `NavAgent`'s target from its
//! `navg.Goal`; `navg.listen` records every `AnimEvent` and `NavArrived` it receives in
//! `navg.Heard`. The engine types are mirrored field for field (ADR-0062: wire layouts come from
//! shapes, and names resolve against the engine's registry).

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_script_guest::{
    EntityId, Module, ModuleDesc, Phase, RunCtx, describe_type, export_module, processor,
};
use serde::{Deserialize, Serialize};

/// Where an agent should walk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "navg.Goal", default)]
pub struct Goal {
    /// The target.
    pub at: Vec3,
}

/// What the entity heard.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "navg.Heard", default)]
pub struct Heard {
    /// Animation events heard, as "name@time in clip".
    pub anim: Vec<String>,
    /// Arrivals heard.
    pub arrived: u32,
    /// Where the last arrival was.
    pub arrived_at: Vec3,
}

/// The engine's `AnimEvent`, mirrored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "AnimEvent", default)]
pub struct AnimEvent {
    /// The event's name.
    pub name: String,
    /// The clip (an asset reference).
    pub clip: String,
    /// The graph node, if any.
    pub node: Option<String>,
    /// The event's time in the clip.
    pub time: f32,
    /// The clip's weight.
    pub weight: f32,
}

/// The engine's `NavArrived`, mirrored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "NavArrived", default)]
pub struct NavArrived {
    /// The target.
    pub target: Vec3,
    /// Where the feet were.
    pub position: Vec3,
    /// Only the closest reachable point.
    pub partial: bool,
}

/// The engine's `NavAgent`, mirrored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "NavAgent", default)]
pub struct NavAgent {
    /// Where to go.
    pub target: Option<Vec3>,
    /// Speed.
    pub max_speed: f32,
    /// Radius.
    pub radius: f32,
    /// Height.
    pub height: f32,
    /// Corner distance.
    pub path_desired_distance: f32,
    /// Arrival distance.
    pub target_desired_distance: f32,
    /// Off-path distance.
    pub path_max_distance: f32,
    /// Volume.
    pub volume: Option<EntityId>,
}

/// The module.
pub struct NavGuest;

impl Module for NavGuest {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "navg".into(),
            version: 1,
            types: Vec::new(),
            components: vec![describe_type::<Goal>(), describe_type::<Heard>()],
            signals: Vec::new(),
            traits: Vec::new(),
            processors: vec![
                processor("navg.go", Phase::PostPhysics)
                    .reads::<Goal>()
                    .writes::<NavAgent>()
                    .build(),
                processor("navg.listen", Phase::PostPhysics)
                    .receives::<AnimEvent>()
                    .receives::<NavArrived>()
                    .writes::<Heard>()
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "navg.go" => go(ctx),
            "navg.listen" => listen(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

fn go(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let goals: Vec<Goal> = ctx.read()?;
    let mut agents: Vec<NavAgent> = ctx.read()?;
    for (agent, goal) in agents.iter_mut().zip(&goals) {
        agent.target = Some(goal.at);
    }
    ctx.write(&agents)
}

fn listen(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let ids = ctx.ids().to_vec();
    let mut heard: Vec<Heard> = ctx.read()?;
    let row = |id: EntityId| ids.iter().position(|i| *i == id);
    for (id, event) in ctx.signals::<AnimEvent>()? {
        if let Some(i) = row(id) {
            heard[i]
                .anim
                .push(format!("{}@{} in {}", event.name, event.time, event.clip));
        }
    }
    for (id, arrived) in ctx.signals::<NavArrived>()? {
        if let Some(i) = row(id) {
            heard[i].arrived += 1;
            heard[i].arrived_at = arrived.position;
        }
    }
    ctx.write(&heard)
}

export_module!(NavGuest);
