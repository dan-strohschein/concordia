//! Clip validation, binding by bone name and sampling per the glTF 2.0 specification (Appendix C).

use std::collections::BTreeSet;

use concordia_assets::{Channel, ChannelPath, ClipData, ClipEvent, Interpolation, validate_events};
use concordia_core::math::{Quat, Vec3};

use crate::error::AnimError;
use crate::pose::Pose;
use crate::skeleton::Skeleton;

/// Checks the duration and every channel: keys with finite, strictly increasing times; finite
/// values of the path's width (three per key for cubic splines); no zero rotations.
pub fn validate_clip(clip: &ClipData) -> Result<(), AnimError> {
    if !(clip.duration.is_finite() && clip.duration >= 0.0) {
        return Err(AnimError::ClipHeader {
            clip: clip.name.clone(),
            reason: format!(
                "duration {} is not a finite, non-negative length",
                clip.duration
            ),
        });
    }
    validate_events(clip).map_err(|reason| AnimError::ClipHeader {
        clip: clip.name.clone(),
        reason,
    })?;
    for (i, channel) in clip.channels.iter().enumerate() {
        check_channel(channel).map_err(|reason| AnimError::Clip {
            clip: clip.name.clone(),
            channel: i,
            target: channel.target_name.clone(),
            reason,
        })?;
    }
    Ok(())
}

/// Values per key for a path (`None`: any width, as morph weights).
fn width(path: ChannelPath) -> Option<usize> {
    match path {
        ChannelPath::Translation | ChannelPath::Scale => Some(3),
        ChannelPath::Rotation => Some(4),
        ChannelPath::Weights => None,
    }
}

fn elements(interpolation: Interpolation) -> usize {
    if interpolation == Interpolation::CubicSpline {
        3
    } else {
        1
    }
}

fn check_channel(c: &Channel) -> Result<(), String> {
    if c.times.is_empty() {
        return Err("no keys".to_owned());
    }
    if c.times.iter().any(|t| !t.is_finite()) || c.values.iter().any(|v| !v.is_finite()) {
        return Err("times and values must be finite".to_owned());
    }
    if c.times.windows(2).any(|w| w[0] >= w[1]) {
        return Err("key times must be strictly increasing".to_owned());
    }
    let per_key = elements(c.interpolation);
    let expected = match width(c.path) {
        Some(w) => c.times.len() * w * per_key,
        None if c.values.len().is_multiple_of(c.times.len() * per_key) => c.values.len(),
        None => return Err(format!("{} weights don't divide into keys", c.values.len())),
    };
    if c.values.len() != expected {
        return Err(format!(
            "{expected} values expected, found {}",
            c.values.len()
        ));
    }
    if c.path == ChannelPath::Rotation && has_zero_rotation(c) {
        return Err("a rotation key is zero (not a rotation)".to_owned());
    }
    Ok(())
}

/// A rotation value (not a cubic tangent) with zero length.
fn has_zero_rotation(c: &Channel) -> bool {
    let per_key = elements(c.interpolation);
    let value = usize::from(per_key == 3);
    c.values
        .chunks(4)
        .enumerate()
        .filter(|(i, _)| i % per_key == value)
        .any(|(_, q)| q.iter().all(|x| *x == 0.0))
}

/// One channel bound to a joint.
#[derive(Debug, Clone, PartialEq)]
struct Track {
    joint: usize,
    path: ChannelPath,
    interpolation: Interpolation,
    times: Vec<f32>,
    values: Vec<f32>,
}

/// A clip bound to one skeleton: its channels resolved to joints by bone name.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundClip {
    name: String,
    duration: f32,
    tracks: Vec<Track>,
    unmatched: Vec<String>,
    skipped: usize,
    events: Vec<ClipEvent>,
    frame_rate: Option<f32>,
}

impl BoundClip {
    /// Validates `clip` and binds its channels to `skeleton`'s joints by `target_name`.
    pub fn bind(clip: &ClipData, skeleton: &Skeleton) -> Result<Self, AnimError> {
        validate_clip(clip)?;
        let mut tracks = Vec::new();
        let mut unmatched = BTreeSet::new();
        let mut skipped = 0;
        for c in &clip.channels {
            if c.path == ChannelPath::Weights {
                skipped += 1;
                continue;
            }
            match skeleton.index_of(&c.target_name) {
                Some(joint) => tracks.push(track(c, joint)),
                None => {
                    unmatched.insert(c.target_name.clone());
                }
            }
        }
        Ok(Self {
            name: clip.name.clone(),
            duration: clip.duration,
            tracks,
            unmatched: unmatched.into_iter().collect(),
            skipped,
            events: clip.events.clone(),
            frame_rate: clip.frame_rate.filter(|r| r.is_finite() && *r > 0.0),
        })
    }

    /// The clip's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Length in seconds.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// Bone names the clip animates that the skeleton lacks (sorted, once each).
    pub fn unmatched(&self) -> &[String] {
        &self.unmatched
    }

    /// Keys per second as authored (FBX), if known.
    pub fn frame_rate(&self) -> Option<f32> {
        self.frame_rate
    }

    /// The clip's events, sorted by time.
    pub fn events(&self) -> &[ClipEvent] {
        &self.events
    }

    /// Channels not sampled (morph target weights).
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// Overwrites the animated parts of `pose` with their values at `time` seconds.
    pub fn sample(&self, time: f32, pose: &mut Pose) -> Result<(), AnimError> {
        let bones = pose.bones.len();
        if let Some(t) = self.tracks.iter().find(|t| t.joint >= bones) {
            return Err(AnimError::PoseSize {
                expected: t.joint + 1,
                got: bones,
            });
        }
        for t in &self.tracks {
            let bone = &mut pose.bones[t.joint];
            match t.path {
                ChannelPath::Translation => bone.translation = Vec3::from_array(vec3(t, time)),
                ChannelPath::Scale => bone.scale = Vec3::from_array(vec3(t, time)),
                ChannelPath::Rotation => bone.rotation = rotation(t, time),
                ChannelPath::Weights => {}
            }
        }
        Ok(())
    }
}

/// A channel as a track, with rotation values (not tangents) normalized.
fn track(c: &Channel, joint: usize) -> Track {
    let mut values = c.values.clone();
    if c.path == ChannelPath::Rotation {
        let per_key = elements(c.interpolation);
        let value = usize::from(per_key == 3);
        for (i, q) in values.chunks_mut(4).enumerate() {
            if i % per_key == value {
                let n = Quat::from_slice(q).normalize();
                q.copy_from_slice(&n.to_array());
            }
        }
    }
    Track {
        joint,
        path: c.path,
        interpolation: c.interpolation,
        times: c.times.clone(),
        values,
    }
}

/// Where `time` falls: the key at or before it and the fraction toward the next key.
enum Span {
    /// Before the first key, after the last, or on a single key: hold key `k`.
    Hold(usize),
    /// Between keys `k` and `k + 1`, `s` of the way (0 ≤ s < 1), `td` seconds apart.
    Between { k: usize, s: f32, td: f32 },
}

fn span(times: &[f32], time: f32) -> Span {
    let last = times.len() - 1;
    if time <= times[0] {
        return Span::Hold(0);
    }
    if time >= times[last] {
        return Span::Hold(last);
    }
    let k = times.partition_point(|t| *t <= time) - 1;
    let td = times[k + 1] - times[k];
    Span::Between {
        k,
        s: (time - times[k]) / td,
        td,
    }
}

/// Element `e` (0 in-tangent, 1 value, 2 out-tangent for cubic; 0 otherwise) of key `k`.
fn key<const W: usize>(t: &Track, k: usize, e: usize) -> [f32; W] {
    let at = (k * elements(t.interpolation) + e) * W;
    std::array::from_fn(|i| t.values[at + i])
}

fn value_index(t: &Track) -> usize {
    usize::from(t.interpolation == Interpolation::CubicSpline)
}

fn vec3(t: &Track, time: f32) -> [f32; 3] {
    match span(&t.times, time) {
        Span::Hold(k) => key::<3>(t, k, value_index(t)),
        Span::Between { k, s, td } => match t.interpolation {
            Interpolation::Step => key::<3>(t, k, 0),
            Interpolation::Linear => {
                let (a, b) = (key::<3>(t, k, 0), key::<3>(t, k + 1, 0));
                std::array::from_fn(|i| a[i] + (b[i] - a[i]) * s)
            }
            Interpolation::CubicSpline => hermite::<3>(t, k, s, td),
        },
    }
}

fn rotation(t: &Track, time: f32) -> Quat {
    match span(&t.times, time) {
        Span::Hold(k) => Quat::from_array(key::<4>(t, k, value_index(t))),
        Span::Between { k, s, td } => match t.interpolation {
            Interpolation::Step => Quat::from_array(key::<4>(t, k, 0)),
            Interpolation::Linear => {
                let a = Quat::from_array(key::<4>(t, k, 0));
                let b = Quat::from_array(key::<4>(t, k + 1, 0));
                a.slerp(b, s).normalize()
            }
            Interpolation::CubicSpline => Quat::from_array(hermite::<4>(t, k, s, td)).normalize(),
        },
    }
}

/// The glTF cubic Hermite spline between keys `k` and `k + 1` (tangents scaled by `td`).
fn hermite<const W: usize>(t: &Track, k: usize, s: f32, td: f32) -> [f32; W] {
    let (s2, s3) = (s * s, s * s * s);
    let v0 = key::<W>(t, k, 1);
    let b0 = key::<W>(t, k, 2);
    let v1 = key::<W>(t, k + 1, 1);
    let a1 = key::<W>(t, k + 1, 0);
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = td * (s3 - 2.0 * s2 + s);
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = td * (s3 - s2);
    std::array::from_fn(|i| h00 * v0[i] + h10 * b0[i] + h01 * v1[i] + h11 * a1[i])
}
