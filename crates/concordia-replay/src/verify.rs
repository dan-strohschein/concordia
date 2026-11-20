//! Comparing a replay with this engine: header fields and checkpoint parts.

use std::collections::BTreeSet;

use concordia_api::{ApiError, ApiErrorKind, Engine, WorldSel};

use crate::format::{Checkpoint, ReplayHeader};
use crate::methods::Mismatch;

/// The header a recording started now would have.
pub(crate) fn current_header(
    engine: &Engine,
    recorded_by: String,
    checkpoint_every: u32,
) -> Result<ReplayHeader, ApiError> {
    let scene = engine
        .scene_path()
        .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))?
        .to_owned();
    let scene_hash = engine
        .world(WorldSel::edit)?
        .state_hash()
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?;
    Ok(ReplayHeader {
        engine: concordia_core::version().to_owned(),
        scene,
        scene_hash: scene_hash.to_string(),
        seed: engine.sim_seed(),
        rate_hz: engine.sim_rate().hz(),
        threads: u32::try_from(engine.threads().get()).unwrap_or(u32::MAX),
        checkpoint_every,
        modules: engine.fingerprints(),
        recorded_by,
    })
}

/// The fields a run must match: engine, scene, scene hash, seed and module code.
pub(crate) fn mismatches(recorded: &ReplayHeader, current: &ReplayHeader) -> Vec<Mismatch> {
    let mut out = Vec::new();
    let mut check = |field: &str, a: String, b: String| {
        if a != b {
            out.push(Mismatch {
                field: field.to_owned(),
                recorded: a,
                current: b,
            });
        }
    };
    check("engine", recorded.engine.clone(), current.engine.clone());
    check("scene", recorded.scene.clone(), current.scene.clone());
    check(
        "scene_hash",
        recorded.scene_hash.clone(),
        current.scene_hash.clone(),
    );
    check("seed", recorded.seed.to_string(), current.seed.to_string());
    if recorded.modules != current.modules {
        out.push(modules_mismatch(recorded, current));
    }
    out
}

/// The module fingerprints that differ, as `module/name@hash` lists.
pub(crate) fn modules_mismatch(recorded: &ReplayHeader, current: &ReplayHeader) -> Mismatch {
    let list = |h: &ReplayHeader| -> BTreeSet<String> {
        h.modules
            .iter()
            .map(|m| format!("{}/{} ({}) {}", m.module, m.name, m.kind, m.hash))
            .collect()
    };
    let (a, b) = (list(recorded), list(current));
    let join = |set: BTreeSet<&String>| set.into_iter().cloned().collect::<Vec<_>>().join(", ");
    Mismatch {
        field: "modules".into(),
        recorded: join(a.difference(&b).collect()),
        current: join(b.difference(&a).collect()),
    }
}

/// The parts whose hash differs between two checkpoints (missing on either side included).
pub(crate) fn diverged_parts(expected: &Checkpoint, actual: &Checkpoint) -> Vec<String> {
    let names: BTreeSet<&String> = expected.parts.keys().chain(actual.parts.keys()).collect();
    names
        .into_iter()
        .filter(|n| expected.parts.get(*n) != actual.parts.get(*n))
        .cloned()
        .collect()
}

/// `ReplayMismatch` listing `mismatches`.
pub(crate) fn mismatch_error(what: &str, mismatches: &[Mismatch]) -> ApiError {
    let fields: Vec<&str> = mismatches.iter().map(|m| m.field.as_str()).collect();
    let mut message = format!("{what}: {} differ", fields.join(", "));
    if let Some(modules) = mismatches.iter().find(|m| m.field == "modules") {
        message.push_str(&format!(
            " (recorded {:?}, now {:?})",
            modules.recorded, modules.current
        ));
    }
    ApiError::new(ApiErrorKind::ReplayMismatch, message).with(
        "mismatches",
        serde_json::to_value(mismatches).unwrap_or_default(),
    )
}
