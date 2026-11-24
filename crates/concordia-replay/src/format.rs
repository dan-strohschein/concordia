//! The `.replay.ron` format, version 1: a header, input changes per tick, checkpoints and every
//! tick's world hash (ADR-0072; M3 phase 3.1a). Version 0 files are read and migrated.

use std::collections::BTreeMap;

use concordia_api::Fingerprint;
use concordia_input::{ActionState, TextInput};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::ReplayError;

/// The format version this crate writes (it also reads version 0).
pub const FORMAT_VERSION: u32 = 1;

/// Hex digits kept of each tick's world hash in [`Replay::hashes`].
pub const TICK_HASH_HEX: usize = 16;

/// Ticks per line of [`Replay::hashes`].
pub const TICKS_PER_LINE: usize = 64;

/// A recorded play session (`.replay.ron`, version 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    /// Format version (1).
    pub version: u32,
    /// What the session ran on.
    pub header: ReplayHeader,
    /// Ticks recorded.
    pub ticks: u64,
    /// The ticks where some action's state changed, with those actions' new states.
    pub inputs: Vec<InputChange>,
    /// World hashes at tick 0, every `checkpoint_every` ticks and the last tick.
    pub checkpoints: Vec<Checkpoint>,
    /// Every tick's world hash after it ran (ticks 1..=`ticks`), the first 16 hex digits each,
    /// 64 ticks per line; empty in files migrated from version 0.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hashes: Vec<String>,
}

impl Replay {
    /// Tick `tick`'s world hash prefix (ticks 1..=`ticks`; none without per-tick hashes).
    pub fn tick_hash(&self, tick: u64) -> Option<&str> {
        if tick == 0 || tick > self.ticks {
            return None;
        }
        let index = usize::try_from(tick - 1).ok()?;
        let line = self.hashes.get(index / TICKS_PER_LINE)?;
        let at = (index % TICKS_PER_LINE) * TICK_HASH_HEX;
        line.get(at..at + TICK_HASH_HEX)
    }
}

/// What a replay was recorded with; a run refuses a different engine, scene, seed or code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ReplayHeader {
    /// Engine version.
    pub engine: String,
    /// OS and CPU, e.g. `macos-aarch64` (replays are exact on one platform; none: migrated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// BLAKE3 of `concordia.toml` (of nothing when absent; none: migrated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_hash: Option<String>,
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
    /// The code and content the engine ran: module fingerprints (e.g. script builds, the
    /// input map) and the template catalog.
    pub modules: Vec<Fingerprint>,
    /// Who recorded it.
    pub recorded_by: String,
    /// The task it is evidence for, e.g. `T-0001`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
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
    /// The DDC key of the world's component values here (a cache for field diffs; none when
    /// recorded without stills).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub still: Option<String>,
}

/// The version 0 format (M2 phase 2.8): version 1 without the fields it added, all optional.
pub const VERSION_0: u32 = 0;

/// Reads and checks replay text (version 1, or 0 migrated to 1): every hash, the per-tick
/// hashes and tick order. A version 0 file has no platform, manifest hash, task or per-tick
/// hashes, so runs don't compare them.
pub fn read_replay(text: &str) -> Result<Replay, ReplayError> {
    let mut replay: Replay = concordia_scene::read_document(text)?;
    match replay.version {
        FORMAT_VERSION => {}
        VERSION_0 => replay.version = FORMAT_VERSION,
        found => return Err(ReplayError::Version { found }),
    }
    check_hash("header.scene_hash", &replay.header.scene_hash)?;
    check_order("inputs", replay.inputs.iter().map(|i| i.tick), replay.ticks)?;
    check_order(
        "checkpoints",
        replay.checkpoints.iter().map(|c| c.tick),
        replay.ticks,
    )?;
    check_tick_hashes(&replay)?;
    for (i, checkpoint) in replay.checkpoints.iter().enumerate() {
        check_hash(&format!("checkpoints[{i}].world"), &checkpoint.world)?;
        if let Some(still) = &checkpoint.still {
            check_hash(&format!("checkpoints[{i}].still"), still)?;
        }
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

/// Per-tick hashes (when present) cover ticks 1..=`ticks`: full lines of 64 ticks, then the
/// rest, 16 hex digits each.
fn check_tick_hashes(replay: &Replay) -> Result<(), ReplayError> {
    if replay.hashes.is_empty() {
        return Ok(());
    }
    let ticks = usize::try_from(replay.ticks).unwrap_or(usize::MAX);
    let lines = ticks.div_ceil(TICKS_PER_LINE);
    let fail = |line: usize, reason: String| Err(ReplayError::TickHashes { line, reason });
    if replay.hashes.len() != lines {
        return fail(
            replay.hashes.len().min(lines),
            format!(
                "{} lines for {ticks} ticks; expected {lines}",
                replay.hashes.len()
            ),
        );
    }
    for (i, line) in replay.hashes.iter().enumerate() {
        let in_line = (ticks - i * TICKS_PER_LINE).min(TICKS_PER_LINE);
        if line.len() != in_line * TICK_HASH_HEX {
            return fail(
                i,
                format!(
                    "{} hex digits; expected {}",
                    line.len(),
                    in_line * TICK_HASH_HEX
                ),
            );
        }
        if let Some(bad) = line.chars().find(|c| !c.is_ascii_hexdigit()) {
            return fail(i, format!("{bad:?} is not a hex digit"));
        }
    }
    Ok(())
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
