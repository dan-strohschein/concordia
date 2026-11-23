//! The `.replay.ron` format, version 0: a header, input changes per tick and checkpoints.

use std::collections::BTreeMap;

use concordia_api::Fingerprint;
use concordia_input::{ActionState, TextInput};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::ReplayError;

/// The format version this crate reads and writes.
pub const FORMAT_VERSION: u32 = 0;

/// A recorded play session (`.replay.ron`, version 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    /// Format version (0).
    pub version: u32,
    /// What the session ran on.
    pub header: ReplayHeader,
    /// Ticks recorded.
    pub ticks: u64,
    /// The ticks where some action's state changed, with those actions' new states.
    pub inputs: Vec<InputChange>,
    /// World hashes at tick 0, every `checkpoint_every` ticks and the last tick.
    pub checkpoints: Vec<Checkpoint>,
}

/// What a replay was recorded with; a run refuses a different engine, scene, seed or code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ReplayHeader {
    /// Engine version.
    pub engine: String,
    /// Project-relative scene path.
    pub scene: String,
    /// The scene's edit-world hash when recording started.
    pub scene_hash: String,
    /// Play-world RNG seed (`[sim] seed`).
    pub seed: u64,
    /// Fixed tick rate.
    pub rate_hz: u32,
    /// Job threads of the recording engine (for information; runs may use any count).
    pub threads: u32,
    /// Ticks between checkpoints.
    pub checkpoint_every: u32,
    /// The code the engine's modules ran (e.g. script builds).
    pub modules: Vec<Fingerprint>,
    /// Who recorded it.
    pub recorded_by: String,
}

/// The actions whose state changed on one tick, and the text typed on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct InputChange {
    /// The tick.
    pub tick: u64,
    /// New states by action name.
    pub actions: BTreeMap<String, ActionState>,
    /// Text typed on the tick, in order (omitted when none).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text: Vec<TextInput>,
}

/// The world's hashes after a tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    /// Ticks completed.
    pub tick: u64,
    /// The world hash (what `sim.step` reports).
    pub world: String,
    /// Hashes by part: each component by name, and `@entities`, `@clock`, `@rng`, `@signals`.
    pub parts: BTreeMap<String, String>,
}

/// Reads and checks replay text: the version, every hash, and tick order.
pub fn read_replay(text: &str) -> Result<Replay, ReplayError> {
    let replay: Replay = concordia_scene::read_document(text)?;
    if replay.version != FORMAT_VERSION {
        return Err(ReplayError::Version {
            found: replay.version,
        });
    }
    check_hash("header.scene_hash", &replay.header.scene_hash)?;
    check_order("inputs", replay.inputs.iter().map(|i| i.tick), replay.ticks)?;
    check_order(
        "checkpoints",
        replay.checkpoints.iter().map(|c| c.tick),
        replay.ticks,
    )?;
    for (i, checkpoint) in replay.checkpoints.iter().enumerate() {
        check_hash(&format!("checkpoints[{i}].world"), &checkpoint.world)?;
        for (part, hash) in &checkpoint.parts {
            check_hash(&format!("checkpoints[{i}].parts.{part}"), hash)?;
        }
    }
    Ok(replay)
}

/// Writes a replay as canonical RON.
pub fn write_replay(replay: &Replay) -> Result<String, ReplayError> {
    Ok(concordia_scene::write_document(replay)?)
}

fn check_hash(at: &str, value: &str) -> Result<(), ReplayError> {
    match value.parse::<concordia_terra::WorldHash>() {
        Ok(_) => Ok(()),
        Err(_) => Err(ReplayError::Hash {
            at: at.to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// Ticks must strictly increase and not pass `ticks` (inputs: below it).
fn check_order(
    list: &str,
    ticks_of: impl Iterator<Item = u64>,
    ticks: u64,
) -> Result<(), ReplayError> {
    let mut last: Option<u64> = None;
    for (i, tick) in ticks_of.enumerate() {
        if last.is_some_and(|l| tick <= l) || tick > ticks {
            return Err(ReplayError::Order {
                at: format!("{list}[{i}]"),
                tick,
                ticks,
            });
        }
        last = Some(tick);
    }
    Ok(())
}
