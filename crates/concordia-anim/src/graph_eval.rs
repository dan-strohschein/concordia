//! One graph evaluation: each node advances by its dt and writes a full pose, returning its
//! playback. Blends are local-space (lerp, shortest-path slerp, lerp); state machines follow
//! Godot's switch and advance modes.

use std::cell::RefCell;
use std::collections::VecDeque;

use concordia_assets::{AdvanceMode, FadeCurve, Parameter, SwitchMode};
use concordia_core::math::{Quat, Vec3};

use crate::events::{AnimEvent, crossed_events};
use crate::graph::{BoundGraph, Edge, Machine, Node};
use crate::graph_player::{AnimGraphPlayer, NodeState};
use crate::player::advance;
use crate::pose::Pose;
use crate::skeleton::Skeleton;

/// Where a node's playback is after this tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Playback {
    /// Seconds into the playing clip.
    pub(crate) position: f32,
    /// Seconds left before the clip's end.
    pub(crate) remaining: f32,
    /// The clip reached its end (or wrapped) this tick.
    pub(crate) ended: bool,
}

/// One clip event crossed this evaluation, before shares are known.
#[derive(Debug, Clone, Copy)]
struct Fired {
    /// How far through its clip's step it was crossed.
    at: f32,
    /// The clip node.
    node: usize,
    /// Index into the clip's events.
    event: usize,
    /// The clip's share of the output pose.
    weight: f32,
}

/// One evaluation's inputs (the graph, the player's parameters and the skeleton) and the clip
/// events it crosses.
pub(crate) struct Eval<'a> {
    graph: &'a BoundGraph,
    player: &'a AnimGraphPlayer,
    skeleton: &'a Skeleton,
    fired: RefCell<Vec<Fired>>,
}

impl<'a> Eval<'a> {
    pub(crate) fn new(
        graph: &'a BoundGraph,
        player: &'a AnimGraphPlayer,
        skeleton: &'a Skeleton,
    ) -> Self {
        Self {
            graph,
            player,
            skeleton,
            fired: RefCell::new(Vec::new()),
        }
    }

    /// The events crossed so far with a share above 0, in firing order: when in the step, then
    /// node (name order), then the clip's own order.
    pub(crate) fn events(self) -> Vec<AnimEvent> {
        let mut fired: Vec<(usize, Fired)> =
            self.fired.into_inner().into_iter().enumerate().collect();
        fired.retain(|(_, f)| f.weight > 0.0);
        fired.sort_by(|(i, a), (j, b)| {
            a.at.total_cmp(&b.at)
                .then(a.node.cmp(&b.node))
                .then(i.cmp(j))
        });
        fired
            .into_iter()
            .filter_map(|(_, f)| {
                let Node::Clip {
                    clip, reference, ..
                } = &self.graph.nodes[f.node]
                else {
                    return None;
                };
                let event = clip.events().get(f.event)?;
                Some(AnimEvent {
                    name: event.name.clone(),
                    clip: reference.clone(),
                    node: Some(self.graph.names[f.node].clone()),
                    time: event.time,
                    weight: f.weight,
                })
            })
            .collect()
    }

    /// Events recorded so far (where a child's begin).
    fn mark(&self) -> usize {
        self.fired.borrow().len()
    }

    /// Multiplies the shares of the events recorded in `range` by `share`.
    fn scale(&self, range: std::ops::Range<usize>, share: f32) {
        if let Some(fired) = self.fired.borrow_mut().get_mut(range) {
            fired.iter_mut().for_each(|f| f.weight *= share);
        }
    }

    /// Node `n` evaluated with `share` of its parent's pose.
    fn shared(
        &self,
        n: usize,
        share: f32,
        dt: f32,
        state: &mut [NodeState],
        out: &mut Pose,
    ) -> Playback {
        let from = self.mark();
        let playback = self.node(n, dt, state, out);
        self.scale(from..self.mark(), share);
        playback
    }

    /// Advances node `n` by `dt` and writes its pose to `out`.
    pub(crate) fn node(
        &self,
        n: usize,
        dt: f32,
        state: &mut [NodeState],
        out: &mut Pose,
    ) -> Playback {
        match &self.graph.nodes[n] {
            Node::Clip { clip, looping, .. } => {
                let s = &mut state[n];
                let duration = clip.duration();
                let crossed = crossed_events(clip.events(), duration, *looping, s.time, dt);
                self.fired
                    .borrow_mut()
                    .extend(crossed.into_iter().map(|c| Fired {
                        at: c.at,
                        node: n,
                        event: c.event,
                        weight: 1.0,
                    }));
                let raw = s.time + dt;
                s.time = advance(s.time, dt, duration, *looping);
                let ended = if *looping {
                    duration > 0.0 && !(0.0..duration).contains(&raw)
                } else if dt >= 0.0 {
                    s.time >= duration
                } else {
                    s.time <= 0.0
                };
                self.skeleton.reset(out);
                let sampled = clip.sample(s.time, out);
                debug_assert!(sampled.is_ok(), "the rest pose fits its skeleton");
                Playback {
                    position: s.time,
                    remaining: duration - s.time,
                    ended,
                }
            }
            Node::Blend2 { a, b, amount, mask } => {
                let w = self.float(amount).clamp(0.0, 1.0);
                // A filtered blend leaves its first input driving every bone outside the filter.
                let share_a = if mask.is_some() { 1.0 } else { 1.0 - w };
                let pa = self.shared(*a, share_a, dt, state, out);
                let mut other = Pose::default();
                let pb = self.shared(*b, w, dt, state, &mut other);
                out.blend_toward(&other, w, mask.as_deref());
                if w > 0.5 { pb } else { pa }
            }
            Node::BlendN { inputs, weights } => self.blend_n(inputs, weights, dt, state, out),
            Node::OneShot { .. } => self.one_shot(n, dt, state, out),
            Node::TimeScale { input, scale } => {
                let scaled = dt * self.float(scale);
                self.node(*input, scaled, state, out)
            }
            Node::Transition { .. } => self.transition(n, dt, state, out),
            Node::Machine(m) => self.machine(n, m, dt, state, out),
        }
    }

    /// Weighted sums of every input, weights clamped to ≥ 0 and normalized (all zero: the first).
    fn blend_n(
        &self,
        inputs: &[usize],
        weights: &[String],
        dt: f32,
        state: &mut [NodeState],
        out: &mut Pose,
    ) -> Playback {
        let mut w: Vec<f32> = weights.iter().map(|p| self.float(p).max(0.0)).collect();
        let sum: f32 = w.iter().sum();
        if sum > 0.0 {
            w.iter_mut().for_each(|x| *x /= sum);
        } else {
            w.iter_mut()
                .enumerate()
                .for_each(|(i, x)| *x = f32::from(u8::from(i == 0)));
        }
        let mut poses = Vec::with_capacity(inputs.len());
        let mut heaviest = (0, f32::MIN, Playback::default());
        for (i, input) in inputs.iter().enumerate() {
            let mut pose = Pose::default();
            let playback = self.shared(*input, w[i], dt, state, &mut pose);
            if w[i] > heaviest.1 {
                heaviest = (i, w[i], playback);
            }
            poses.push(pose);
        }
        weighted_sum(&poses, &w, out);
        heaviest.2
    }

    /// Godot's one-shot: fires on its trigger, fades in from the start and out to the shot's end.
    fn one_shot(&self, n: usize, dt: f32, state: &mut [NodeState], out: &mut Pose) -> Playback {
        let Node::OneShot {
            base,
            shot,
            request,
            fade_in,
            fade_out,
            curve,
            mask,
        } = &self.graph.nodes[n]
        else {
            unreachable!("called for one-shots");
        };
        if self.fired(request) {
            self.reset(*shot, state);
            state[n].active = true;
            state[n].time = 0.0;
        }
        let base_events = self.mark();
        let playback = self.node(*base, dt, state, out);
        if !state[n].active {
            return playback;
        }
        state[n].time += dt.abs();
        let mut overlay = Pose::default();
        let shot_events = self.mark();
        let shot_playback = self.node(*shot, dt, state, &mut overlay);
        let w_in = ramp(state[n].time, *fade_in);
        let w_out = ramp(shot_playback.remaining, *fade_out);
        let w = shape(*curve, w_in.min(w_out));
        // Shares are the node's blend this tick (also on the tick the shot ends).
        self.scale(
            base_events..shot_events,
            if mask.is_some() { 1.0 } else { 1.0 - w },
        );
        self.scale(shot_events..self.mark(), w);
        if shot_playback.ended {
            state[n].active = false;
            return playback;
        }
        out.blend_toward(&overlay, w, mask.as_deref());
        playback
    }

    /// Godot's transition node: switches to the input its int parameter names, cross-fading.
    fn transition(&self, n: usize, dt: f32, state: &mut [NodeState], out: &mut Pose) -> Playback {
        let Node::Transition {
            inputs,
            select,
            xfade,
            curve,
        } = &self.graph.nodes[n]
        else {
            unreachable!("called for transitions");
        };
        let selected = u32::try_from(self.int(select))
            .ok()
            .filter(|i| (*i as usize) < inputs.len());
        if let Some(to) = selected.filter(|to| *to != state[n].current) {
            let s = &mut state[n];
            s.previous = (*xfade > 0.0).then_some(s.current);
            s.fade = 0.0;
            s.current = to;
            let (node, reset) = inputs[to as usize];
            if reset {
                self.reset(node, state);
            }
        }
        let current = inputs[state[n].current as usize].0;
        let destination = self.mark();
        let playback = self.node(current, dt, state, out);
        if let Some(previous) = state[n].previous {
            let previous = inputs[previous as usize].0;
            let source = self.mark();
            let share = self.fade_from(n, previous, *xfade, *curve, dt, state, out);
            self.scale(destination..source, share);
        }
        playback
    }

    /// Blends `out` (the destination) over the fading-out node, or ends the fade; returns the
    /// destination's share (the source's events get the rest).
    #[allow(clippy::too_many_arguments, reason = "one fade step's inputs")]
    fn fade_from(
        &self,
        n: usize,
        from: usize,
        xfade: f32,
        curve: FadeCurve,
        dt: f32,
        state: &mut [NodeState],
        out: &mut Pose,
    ) -> f32 {
        state[n].fade += dt.abs();
        let w = if xfade > 0.0 {
            state[n].fade / xfade
        } else {
            1.0
        };
        if w >= 1.0 {
            state[n].previous = None;
            state[n].via = None;
            return 1.0;
        }
        let share = shape(curve, w);
        let mut old = Pose::default();
        self.shared(from, 1.0 - share, dt, state, &mut old);
        old.blend_toward(out, share, None);
        std::mem::swap(out, &mut old);
        share
    }

    /// Godot's state machine: resolve travel, take at most one transition, play the state.
    fn machine(
        &self,
        n: usize,
        m: &Machine,
        dt: f32,
        state: &mut [NodeState],
        out: &mut Pose,
    ) -> Playback {
        self.request_travel(n, m, state);
        if let Some(e) = self.choose(m, &mut state[n]) {
            self.take(n, m, e, state);
        }
        let current = m.nodes[state[n].current as usize];
        let destination = self.mark();
        let playback = self.node(current, dt, state, out);
        if let (Some(previous), Some(via)) = (state[n].previous, state[n].via) {
            let edge = &m.edges[via as usize];
            let from = m.nodes[previous as usize];
            let source = self.mark();
            let share = self.fade_from(n, from, edge.xfade, edge.curve, dt, state, out);
            self.scale(destination..source, share);
        }
        state[n].ended = playback.ended;
        state[n].position = playback.position;
        playback
    }

    /// A travel request becomes a path, or a teleport when the target can't be reached.
    fn request_travel(&self, n: usize, m: &Machine, state: &mut [NodeState]) {
        let Some(target) = self.player.travel.get(&self.graph.names[n]) else {
            return;
        };
        let Some(to) = m.states.iter().position(|s| s == target) else {
            return;
        };
        let to = u32::try_from(to).unwrap_or(0);
        let s = &mut state[n];
        if to == s.current {
            s.path.clear();
            return;
        }
        match travel_path(m, s.current, to) {
            Some(path) => s.path = path,
            None => {
                s.current = to;
                s.previous = None;
                s.via = None;
                s.path.clear();
                s.ended = false;
                self.reset(m.nodes[to as usize], state);
            }
        }
    }

    /// The transition to take this tick: the next travel step, else the first `Auto` one whose
    /// condition holds; switch modes permitting.
    fn choose(&self, m: &Machine, s: &mut NodeState) -> Option<u32> {
        let allowed = |e: &Edge| e.switch != SwitchMode::AtEnd || s.ended;
        if let Some(&next) = s.path.first() {
            let edge = &m.edges[next as usize];
            if edge.from != s.current {
                s.path.clear();
            } else if allowed(edge) {
                s.path.remove(0);
                return Some(next);
            } else {
                return None;
            }
        }
        m.by_priority.iter().copied().find(|e| {
            let edge = &m.edges[*e as usize];
            edge.from == s.current
                && edge.advance == AdvanceMode::Auto
                && edge.condition.as_deref().is_none_or(|c| self.condition(c))
                && allowed(edge)
        })
    }

    /// Enters edge `e`'s destination: the old state fades out; the new one resets or syncs.
    fn take(&self, n: usize, m: &Machine, e: u32, state: &mut [NodeState]) {
        let edge = &m.edges[e as usize];
        let s = &mut state[n];
        let position = s.position;
        s.previous = (edge.xfade > 0.0).then_some(s.current);
        s.via = s.previous.map(|_| e);
        s.fade = 0.0;
        s.current = edge.to;
        s.ended = false;
        let to = m.nodes[edge.to as usize];
        if edge.switch == SwitchMode::Sync {
            self.seek(to, position, state);
        } else if edge.reset {
            self.reset(to, state);
        }
    }

    /// Node `n` and everything it plays back to their starting playback.
    fn reset(&self, n: usize, state: &mut [NodeState]) {
        state[n] = self.graph.initial(n);
        for child in self.graph.nodes[n].children() {
            self.reset(child, state);
        }
    }

    /// Every clip under node `n` to `position` seconds.
    fn seek(&self, n: usize, position: f32, state: &mut [NodeState]) {
        if let Node::Clip { clip, looping, .. } = &self.graph.nodes[n] {
            state[n].time = advance(0.0, position, clip.duration(), *looping);
        }
        for child in self.graph.nodes[n].children() {
            self.seek(child, position, state);
        }
    }

    fn float(&self, name: &str) -> f32 {
        let default = match self.graph.parameters.get(name) {
            Some(Parameter::Float(v)) => *v,
            _ => 0.0,
        };
        self.player
            .floats
            .get(name)
            .copied()
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }

    fn int(&self, name: &str) -> i32 {
        let default = match self.graph.parameters.get(name) {
            Some(Parameter::Int(v)) => *v,
            _ => 0,
        };
        self.player.ints.get(name).copied().unwrap_or(default)
    }

    fn fired(&self, trigger: &str) -> bool {
        self.player.triggers.iter().any(|t| t == trigger)
    }

    /// A bool parameter's value, or whether a trigger fired.
    fn condition(&self, name: &str) -> bool {
        match self.graph.parameters.get(name) {
            Some(Parameter::Bool(default)) => {
                self.player.bools.get(name).copied().unwrap_or(*default)
            }
            Some(Parameter::Trigger) => self.fired(name),
            _ => false,
        }
    }
}

impl Node {
    /// The nodes this one plays.
    pub(crate) fn children(&self) -> Vec<usize> {
        match self {
            Self::Clip { .. } => Vec::new(),
            Self::Blend2 { a, b, .. } => vec![*a, *b],
            Self::BlendN { inputs, .. } => inputs.clone(),
            Self::OneShot { base, shot, .. } => vec![*base, *shot],
            Self::TimeScale { input, .. } => vec![*input],
            Self::Transition { inputs, .. } => inputs.iter().map(|(n, _)| *n).collect(),
            Self::Machine(m) => m.nodes.clone(),
        }
    }
}

/// The fewest transitions from `from` to `to` that aren't disabled; ties go to the path whose
/// steps come first in (priority, file order). `None` if there is no path.
fn travel_path(m: &Machine, from: u32, to: u32) -> Option<Vec<u32>> {
    let mut via: Vec<Option<u32>> = vec![None; m.states.len()];
    let mut seen = vec![false; m.states.len()];
    seen[from as usize] = true;
    let mut queue = VecDeque::from([from]);
    while let Some(state) = queue.pop_front() {
        for &e in &m.by_priority {
            let edge = &m.edges[e as usize];
            if edge.from == state
                && edge.advance != AdvanceMode::Disabled
                && !seen[edge.to as usize]
            {
                seen[edge.to as usize] = true;
                via[edge.to as usize] = Some(e);
                queue.push_back(edge.to);
            }
        }
    }
    if !seen[to as usize] {
        return None;
    }
    let mut path = Vec::new();
    let mut at = to;
    while let Some(e) = via[at as usize] {
        path.push(e);
        at = m.edges[e as usize].from;
    }
    path.reverse();
    Some(path)
}

/// `x / length` clamped to 0..=1 (1 for no fade).
fn ramp(x: f32, length: f32) -> f32 {
    if length > 0.0 {
        (x / length).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// A fade's progress shaped by its curve.
fn shape(curve: FadeCurve, w: f32) -> f32 {
    match curve {
        FadeCurve::Linear => w,
        FadeCurve::Smooth => w * w * (3.0 - 2.0 * w),
    }
}

/// `out` = the normalized weighted sum of `poses`: translations and scales summed, rotations
/// summed in the first pose's hemisphere and normalized.
fn weighted_sum(poses: &[Pose], weights: &[f32], out: &mut Pose) {
    let Some(first) = poses.first() else {
        return;
    };
    out.bones.clone_from(&first.bones);
    for (j, bone) in out.bones.iter_mut().enumerate() {
        let reference = first.bones[j].rotation;
        let (mut t, mut s, mut r) = (Vec3::ZERO, Vec3::ZERO, Quat::from_xyzw(0.0, 0.0, 0.0, 0.0));
        for (pose, w) in poses.iter().zip(weights) {
            let b = &pose.bones[j];
            t += b.translation * *w;
            s += b.scale * *w;
            let q = if b.rotation.dot(reference) < 0.0 {
                -b.rotation
            } else {
                b.rotation
            };
            r += q * *w;
        }
        bone.translation = t;
        bone.scale = s;
        bone.rotation = if r.length_squared() > 0.0 {
            r.normalize()
        } else {
            reference
        };
    }
}
