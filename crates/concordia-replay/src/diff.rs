//! Comparing two replay files offline: header fields, the first differing tick, the parts and,
//! from the stills in the cache, the fields.

use std::path::Path;

use crate::format::Replay;
use crate::methods::{FieldDiff, Mismatch};
use crate::still;
use crate::verify::{diverged_parts, mismatches};

/// How two replays differ.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct ReplayDiff {
    /// Header fields that differ.
    pub mismatches: Vec<Mismatch>,
    /// The first tick whose recorded input differs.
    pub first_input_tick: Option<u64>,
    /// The first tick whose hash differs.
    pub first_tick: Option<u64>,
    /// The checkpoint the parts are compared at.
    pub parts_tick: Option<u64>,
    /// Differing parts there.
    pub parts: Vec<String>,
    /// Differing values there.
    pub fields: Vec<FieldDiff>,
    /// Why `fields` is empty although parts differ.
    pub fields_note: Option<String>,
}

impl ReplayDiff {
    /// Nothing differs.
    pub fn same(&self) -> bool {
        self.mismatches.is_empty() && self.first_input_tick.is_none() && self.first_tick.is_none()
    }
}

/// Compares `a` (expected) with `b`, reading stills from the DDC at `ddc`: header fields, the
/// first tick whose hash differs (per-tick hashes, else common checkpoints), and the parts and
/// fields at the first common checkpoint from there.
pub fn compare_replays(a: &Replay, b: &Replay, ddc: &Path) -> ReplayDiff {
    let mut out = ReplayDiff {
        mismatches: mismatches(&a.header, &b.header),
        first_input_tick: first_input_tick(a, b),
        first_tick: first_tick(a, b),
        ..ReplayDiff::default()
    };
    let Some(first) = out.first_tick else {
        return out;
    };
    let at = a
        .checkpoints
        .iter()
        .filter(|c| c.tick >= first)
        .find_map(|x| {
            b.checkpoints
                .iter()
                .find(|y| y.tick == x.tick)
                .map(|y| (x, y))
        });
    let Some((x, y)) = at else {
        return out;
    };
    out.parts_tick = Some(x.tick);
    out.parts = diverged_parts(x, y);
    match (&x.still, &y.still) {
        (Some(kx), Some(ky)) => match (still::load(ddc, kx), still::load(ddc, ky)) {
            (Some(sx), Some(sy)) => out.fields = still::diff(&sx, &sy),
            _ => out.fields_note = Some(format!("a still for tick {} is not in the cache", x.tick)),
        },
        _ => out.fields_note = Some("one of the replays was recorded without stills".into()),
    }
    out
}

/// The first tick where one replay's input change differs from the other's (or only one has
/// one).
fn first_input_tick(a: &Replay, b: &Replay) -> Option<u64> {
    let ticks: std::collections::BTreeSet<u64> =
        a.inputs.iter().chain(&b.inputs).map(|i| i.tick).collect();
    fn at(r: &Replay, t: u64) -> Option<&crate::format::InputChange> {
        r.inputs.iter().find(|i| i.tick == t)
    }
    ticks.into_iter().find(|&t| at(a, t) != at(b, t))
}

/// The first tick (or common checkpoint, without per-tick hashes on both) that differs.
fn first_tick(a: &Replay, b: &Replay) -> Option<u64> {
    if !a.hashes.is_empty() && !b.hashes.is_empty() {
        let last = a.ticks.min(b.ticks);
        return (1..=last)
            .find(|&t| a.tick_hash(t) != b.tick_hash(t))
            .or_else(|| (a.ticks != b.ticks).then_some(last + 1));
    }
    a.checkpoints
        .iter()
        .filter_map(|x| {
            b.checkpoints
                .iter()
                .find(|y| y.tick == x.tick)
                .map(|y| (x, y))
        })
        .find(|(x, y)| x.world != y.world)
        .map(|(x, _)| x.tick)
}
