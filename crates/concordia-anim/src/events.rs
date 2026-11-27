//! Clip events (M2 phase 2.4.4): which events a playback step crosses, and the `AnimEvent` signal
//! raised for each, delivered on the next tick (ADR-0069).

use concordia_assets::{AssetRef, ClipEvent};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Raised on a skinned entity when its playback crosses a clip event; delivered next tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "Animation")]
#[serde(deny_unknown_fields)]
pub struct AnimEvent {
    /// The event's name.
    pub name: String,
    /// The clip it belongs to.
    pub clip: AssetRef,
    /// The graph's clip node that played it (none for an `AnimPlayer`).
    pub node: Option<String>,
    /// The event's time in the clip, in seconds.
    pub time: f32,
    /// The clip's share of the pose when it fired (1 for an `AnimPlayer`).
    pub weight: f32,
}

/// One event a step crossed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    /// Index into the clip's events.
    pub event: usize,
    /// How far through the step it was crossed (0 at the start, 1 at the end).
    pub at: f32,
}

/// The events crossed by moving `delta` seconds from `time` through a clip of `duration`, in
/// crossing order: forward steps cover `[from, to)`, backward ones `(to, from]`; a clamped clip
/// that reaches its end also covers it; looping steps fire once per lap.
pub fn crossed_events(
    events: &[ClipEvent],
    duration: f32,
    looping: bool,
    time: f32,
    delta: f32,
) -> Vec<Crossing> {
    let usable = duration.is_finite() && duration > 0.0 && time.is_finite() && delta.is_finite();
    // A zero step covers nothing: an empty range in both helpers.
    if events.is_empty() || !usable {
        return Vec::new();
    }
    let mut found = if looping {
        laps(events, duration, time.rem_euclid(duration), delta)
    } else {
        clamped(events, duration, time.clamp(0.0, duration), delta)
    };
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    found
        .into_iter()
        .map(|(at, _, event)| Crossing { event, at })
        .collect()
}

/// Most laps one step may cross (a guard against degenerate clips; far beyond any real step).
const MAX_LAPS: f32 = 10_000.0;

/// (fraction, lap, event) for a looping step: events repeat every `duration`, unwrapped.
fn laps(events: &[ClipEvent], duration: f32, from: f32, delta: f32) -> Vec<(f32, u32, usize)> {
    let len = delta.abs();
    let laps = (len / duration).ceil().min(MAX_LAPS) as u32 + 1;
    let mut out = Vec::new();
    for lap in 0..=laps {
        let base = lap as f32 * duration;
        for (i, e) in events.iter().enumerate() {
            // How far along the step the event's copy in this lap lies.
            let d = if delta > 0.0 {
                e.time + base - from
            } else {
                from - (e.time - base)
            };
            if (0.0..len).contains(&d) {
                out.push((d / len, lap, i));
            }
        }
    }
    out
}

/// (fraction, 0, event) for a clamped step, which also covers the end it reaches.
fn clamped(events: &[ClipEvent], duration: f32, from: f32, delta: f32) -> Vec<(f32, u32, usize)> {
    let len = delta.abs();
    let (end, reached) = if delta > 0.0 {
        ((from + len).min(duration), from + len >= duration)
    } else {
        ((from - len).max(0.0), from - len <= 0.0)
    };
    if end == from {
        return Vec::new();
    }
    let limit = if delta > 0.0 { duration } else { 0.0 };
    events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let d = if delta > 0.0 {
                e.time - from
            } else {
                from - e.time
            };
            let inside = d >= 0.0 && d < (end - from).abs();
            (inside || (reached && e.time == limit)).then_some((d / len, 0, i))
        })
        .collect()
}
