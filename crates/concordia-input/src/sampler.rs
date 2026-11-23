//! Deterministic sampling: queued events become action states once per fixed tick.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use concordia_core::math::{Vec2, Vec3};
use concordia_core::time::TickRate;

use crate::device::{KeyCode, MouseButton};
use crate::error::InputError;
use crate::event::InputEvent;
use crate::map::{ActionDef, ActionKind, Binding, InputMap};
use crate::text::TextInput;
use crate::value::{ActionState, ActionStates, ActionValue};

/// The tick an OS event at `since_start` belongs to: events in (N, N+1] tick times apply at tick N+1.
pub fn tick_for_time(since_start: Duration, rate: TickRate) -> u64 {
    let nanos = since_start.as_nanos() * u128::from(rate.hz());
    u64::try_from(nanos.div_ceil(1_000_000_000)).unwrap_or(u64::MAX)
}

/// Held keys and buttons, pointer position, and this tick's motion and scroll.
#[derive(Debug, Clone, Default, PartialEq)]
struct Devices {
    keys: BTreeSet<KeyCode>,
    buttons: BTreeSet<MouseButton>,
    pointer: Vec2,
    ray: Option<ActionValue>,
    motion: Vec2,
    scroll: Vec2,
    viewport: Vec2,
}

/// Turns queued events into [`ActionStates`] once per tick; a play-world resource.
#[derive(Debug, Clone, PartialEq)]
pub struct InputSampler {
    map: InputMap,
    devices: Devices,
    forced: BTreeMap<String, ActionValue>,
    queue: BTreeMap<u64, Vec<InputEvent>>,
    states: ActionStates,
    typed: Vec<TextInput>,
    next_tick: u64,
    playback: Option<Playback>,
}

/// Recorded action-state changes by tick, played back until `until`.
#[derive(Debug, Clone, PartialEq)]
struct Playback {
    changes: BTreeMap<u64, BTreeMap<String, ActionState>>,
    text: BTreeMap<u64, Vec<TextInput>>,
    until: u64,
}

impl InputSampler {
    /// A sampler for a validated map, before tick 0.
    pub fn new(map: InputMap) -> Self {
        let states = ActionStates {
            actions: map
                .actions
                .iter()
                .map(|(name, def)| (name.clone(), idle(def.kind)))
                .collect(),
        };
        Self {
            map,
            devices: Devices::default(),
            forced: BTreeMap::new(),
            queue: BTreeMap::new(),
            states,
            typed: Vec::new(),
            next_tick: 0,
            playback: None,
        }
    }

    /// The input map.
    pub fn map(&self) -> &InputMap {
        &self.map
    }

    /// The first tick that can still take input.
    pub fn next_tick(&self) -> u64 {
        self.next_tick
    }

    /// The states computed by the last [`sample`](Self::sample).
    pub fn states(&self) -> &ActionStates {
        &self.states
    }

    /// The text typed on the last sampled tick, in order.
    pub fn typed(&self) -> &[TextInput] {
        &self.typed
    }

    /// Checks that `event` could be queued for `tick`, without queuing it.
    pub fn check(&self, tick: u64, event: &InputEvent) -> Result<(), InputError> {
        if tick < self.next_tick {
            return Err(InputError::TickInPast {
                tick,
                next: self.next_tick,
            });
        }
        match event {
            InputEvent::Action { action, value } => {
                self.check_action(action, *value)?;
                ray_value(*value).map(|_| ())
            }
            InputEvent::PointerRay { origin, direction } => {
                normalized_ray(*origin, *direction).map(|_| ())
            }
            InputEvent::Viewport { width, height } => check_viewport(*width, *height),
            InputEvent::Text(text) => text.check(),
            _ => Ok(()),
        }
    }

    /// Queues `event` for `tick` (events of one tick apply in push order).
    pub fn push(&mut self, tick: u64, event: InputEvent) -> Result<(), InputError> {
        self.check(tick, &event)?;
        self.queue.entry(tick).or_default().push(event);
        Ok(())
    }

    /// Plays recorded action states back (e.g. a replay): until tick `until`, each tick's states
    /// are the previous tick's with that tick's `changes` applied, and device events queued for
    /// those ticks are dropped. Every action must exist with a value of its kind.
    pub fn play_back(
        &mut self,
        changes: BTreeMap<u64, BTreeMap<String, ActionState>>,
        until: u64,
    ) -> Result<(), InputError> {
        for (action, state) in changes.values().flatten() {
            self.check_action(action, state.value)?;
        }
        self.playback = Some(Playback {
            changes,
            text: BTreeMap::new(),
            until,
        });
        Ok(())
    }

    /// Recorded typed text by tick, played back with the action changes (call after
    /// [`play_back`](Self::play_back)).
    pub fn play_back_text(&mut self, text: BTreeMap<u64, Vec<TextInput>>) {
        if let Some(p) = self.playback.as_mut() {
            p.text = text;
        }
    }

    /// The tick's states from the playback, if `tick` is one it covers.
    fn play(&mut self, tick: u64) -> Option<&ActionStates> {
        let playback = self.playback.as_mut().filter(|p| tick < p.until)?;
        self.queue.retain(|t, _| *t > tick);
        if let Some(changes) = playback.changes.remove(&tick) {
            self.states.actions.extend(changes);
        }
        self.typed = playback.text.remove(&tick).unwrap_or_default();
        self.next_tick = self.next_tick.max(tick.saturating_add(1));
        Some(&self.states)
    }

    /// Applies every event queued up to `tick` and returns the tick's action states.
    pub fn sample(&mut self, tick: u64) -> &ActionStates {
        if self.play(tick).is_some() {
            return &self.states;
        }
        let before: BTreeMap<String, bool> = self.active_now();
        self.typed.clear();
        let mut rose = BTreeSet::new();
        let mut fell = BTreeSet::new();
        self.devices.motion = Vec2::ZERO;
        self.devices.scroll = Vec2::ZERO;
        let due: Vec<u64> = self.queue.range(..=tick).map(|(t, _)| *t).collect();
        let events: Vec<InputEvent> = due
            .into_iter()
            .flat_map(|t| self.queue.remove(&t).unwrap_or_default())
            .collect();
        for event in events {
            let was = self.active_now();
            self.apply(event);
            for (name, now) in self.active_now() {
                match (was.get(&name).copied().unwrap_or(false), now) {
                    (false, true) => rose.insert(name),
                    (true, false) => fell.insert(name),
                    _ => false,
                };
            }
        }
        self.states = self.compute(&before, &rose, &fell);
        self.next_tick = self.next_tick.max(tick.saturating_add(1));
        &self.states
    }

    fn check_action(&self, action: &str, value: ActionValue) -> Result<(), InputError> {
        let def = self
            .map
            .action(action)
            .ok_or_else(|| InputError::UnknownAction {
                action: action.to_owned(),
                known: self.map.action_names().map(str::to_owned).collect(),
            })?;
        match value.kind() == def.kind {
            true => Ok(()),
            false => Err(InputError::ValueKind {
                action: action.to_owned(),
                kind: format!("{:?}", def.kind),
            }),
        }
    }

    fn apply(&mut self, event: InputEvent) {
        if let InputEvent::Text(text) = event {
            self.typed.push(text);
            return;
        }
        let d = &mut self.devices;
        match event {
            InputEvent::Key { key, pressed } => toggle(&mut d.keys, key, pressed),
            InputEvent::MouseButton { button, pressed } => toggle(&mut d.buttons, button, pressed),
            InputEvent::PointerMove { x, y } => d.pointer = Vec2::new(x, y),
            InputEvent::PointerRay { origin, direction } => {
                d.ray = normalized_ray(origin, direction).ok().or(d.ray);
            }
            InputEvent::MouseMotion { dx, dy } => d.motion += Vec2::new(dx, dy),
            InputEvent::Scroll { dx, dy } => d.scroll += Vec2::new(dx, dy),
            InputEvent::Viewport { width, height } => d.viewport = Vec2::new(width, height),
            InputEvent::Text(_) => {}
            InputEvent::Action { action, value } => {
                let value = ray_value(value).unwrap_or(value);
                self.forced.insert(action, value);
            }
        }
    }

    fn active_now(&self) -> BTreeMap<String, bool> {
        self.map
            .actions
            .iter()
            .map(|(name, def)| (name.clone(), self.value(name, def).is_active()))
            .collect()
    }

    fn compute(
        &self,
        before: &BTreeMap<String, bool>,
        rose: &BTreeSet<String>,
        fell: &BTreeSet<String>,
    ) -> ActionStates {
        let actions = self
            .map
            .actions
            .iter()
            .map(|(name, def)| {
                let value = self.value(name, def);
                let pressed = value.is_active();
                let was = before.get(name).copied().unwrap_or(false);
                let state = ActionState {
                    pressed,
                    just_pressed: rose.contains(name) || (!was && pressed),
                    just_released: fell.contains(name) || (was && !pressed),
                    value,
                };
                (name.clone(), state)
            })
            .collect();
        ActionStates { actions }
    }

    fn value(&self, name: &str, def: &ActionDef) -> ActionValue {
        let bound = bound_value(&self.devices, def);
        match self.forced.get(name) {
            Some(forced) => merge(bound, *forced),
            None => bound,
        }
    }
}

/// A ray value with its direction normalized; rejects non-finite or zero rays.
fn normalized_ray(origin: Vec3, direction: Vec3) -> Result<ActionValue, InputError> {
    let unit = direction.try_normalize().filter(|_| origin.is_finite());
    unit.map(|direction| ActionValue::Ray { origin, direction })
        .ok_or_else(|| {
            InputError::Pointer(format!(
                "ray from {origin} along {direction}: both must be finite and the direction non-zero"
            ))
        })
}

/// A viewport must be finite and positive.
fn check_viewport(width: f32, height: f32) -> Result<(), InputError> {
    match [width, height].iter().all(|v| v.is_finite() && *v > 0.0) {
        true => Ok(()),
        false => Err(InputError::Pointer(format!(
            "viewport {width} x {height}: both must be finite and positive"
        ))),
    }
}

/// `value` normalized if it is a ray (other kinds pass through).
fn ray_value(value: ActionValue) -> Result<ActionValue, InputError> {
    match value {
        ActionValue::Ray { origin, direction } => normalized_ray(origin, direction),
        other => Ok(other),
    }
}

fn idle(kind: ActionKind) -> ActionState {
    ActionState {
        pressed: false,
        just_pressed: false,
        just_released: false,
        value: ActionValue::zero(kind),
    }
}

fn toggle<T: Ord>(set: &mut BTreeSet<T>, item: T, on: bool) {
    match on {
        true => set.insert(item),
        false => set.remove(&item),
    };
}

fn bound_value(d: &Devices, def: &ActionDef) -> ActionValue {
    match def.kind {
        ActionKind::Button => ActionValue::Button(def.bindings.iter().any(|b| held(d, b))),
        ActionKind::Axis1d => ActionValue::Axis1d(def.bindings.iter().map(|b| axis1d(d, b)).sum()),
        ActionKind::Axis2d => ActionValue::Axis2d(def.bindings.iter().map(|b| axis2d(d, b)).sum()),
        ActionKind::Ray => def
            .bindings
            .iter()
            .find_map(|b| (*b == Binding::PointerRay).then_some(d.ray).flatten())
            .unwrap_or(ActionValue::zero(ActionKind::Ray)),
    }
}

fn held(d: &Devices, binding: &Binding) -> bool {
    match binding {
        Binding::Key(k) => d.keys.contains(k),
        Binding::MouseButton(b) => d.buttons.contains(b),
        Binding::Chord { modifiers, key } => {
            d.keys.contains(key)
                && modifiers
                    .iter()
                    .all(|m| d.keys.iter().any(|k| k.modifier() == Some(*m)))
        }
        _ => false,
    }
}

fn axis1d(d: &Devices, binding: &Binding) -> f32 {
    let key = |k: &KeyCode| if d.keys.contains(k) { 1.0 } else { 0.0 };
    match binding {
        Binding::Key(k) => key(k),
        Binding::KeyAxis { negative, positive } => key(positive) - key(negative),
        Binding::Scroll => d.scroll.y,
        _ => 0.0,
    }
}

fn axis2d(d: &Devices, binding: &Binding) -> Vec2 {
    let key = |k: &KeyCode| if d.keys.contains(k) { 1.0 } else { 0.0 };
    match binding {
        Binding::KeyAxis2d {
            up,
            down,
            left,
            right,
        } => Vec2::new(key(right) - key(left), key(up) - key(down)).clamp_length_max(1.0),
        Binding::PointerPosition => d.pointer,
        Binding::MouseDelta => d.motion,
        Binding::Scroll => d.scroll,
        Binding::Viewport => d.viewport,
        _ => Vec2::ZERO,
    }
}

/// A forced value wins for buttons when pressed and replaces axis values.
fn merge(bound: ActionValue, forced: ActionValue) -> ActionValue {
    match (bound, forced) {
        (ActionValue::Button(a), ActionValue::Button(b)) => ActionValue::Button(a || b),
        (_, forced) => forced,
    }
}
