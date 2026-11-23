//! `InputActions` (M2 phase 2.10a, gap S1): gameplay scripts see only components, so
//! `input.actions` mirrors each entity's watched actions into its `InputActions` every tick.
//!
//! The mirror's rules, from which every expected value below is derived by hand:
//! - consumed this tick (the UI took the pointer or the keyboard): released, the kind's zero
//!   value, and `just_released` exactly when the mirror showed it pressed last tick;
//! - otherwise: the raw `pressed` and value; `just_pressed` when the raw state says so or the
//!   mirror showed it released last tick; `just_released` when the mirror showed it pressed and
//!   it is released now, or when it was pressed and released within this tick (a tap);
//! - unknown or unwatched names are absent.

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{ApiError, Engine, EngineConfig, EngineModule, PlaySetup, WorldSel};
use concordia_core::id::SessionId;
use concordia_core::math::Vec2;
use concordia_input::{
    ActionState, ActionValue, ConsumedActions, InputActions, InputMap, InputModule, mirror_action,
};
use concordia_terra::{Phase, Schedule, World, system};
use serde_json::{Value, json};

const DEFAULT: &str = include_str!("../../../examples/m1-physics-stack/input/default.input.ron");

/// A stand-in for the UI: a `ui.interact` system that takes the keyboard on chosen ticks.
#[derive(Debug)]
struct KeyTaker {
    ticks: Vec<u64>,
}

impl EngineModule for KeyTaker {
    fn name(&self) -> &'static str {
        "key-taker"
    }

    fn start_play(&self, _: &mut World) -> Result<PlaySetup, ApiError> {
        let ticks = self.ticks.clone();
        let mut schedule = Schedule::new();
        schedule.add_in(
            Phase::PrePhysics,
            system("ui.interact", move |world: &mut World, ctx| {
                if ticks.contains(&ctx.tick.0)
                    && let Some(c) = world.resource_mut::<ConsumedActions>()
                {
                    c.consume_keys();
                }
            })
            .after_if_present("input.sample"),
        );
        Ok(PlaySetup {
            schedule,
            state: None,
        })
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    fn new(extra: Vec<Arc<dyn EngineModule>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(7);
        config.threads = NonZeroUsize::MIN;
        config.journal.sync = false;
        let mut modules: Vec<Arc<dyn EngineModule>> = vec![Arc::new(InputModule::new(
            InputMap::parse(DEFAULT).unwrap(),
        ))];
        modules.extend(extra);
        config.modules = modules;
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut rig = Self {
            _dir: dir,
            engine,
            session,
        };
        rig.ok(
            "session.open",
            json!({"client": "test", "actor": "agent:test"}),
        );
        rig.ok(
            "scene.new",
            json!({"path": "scenes/t.scene.ron", "name": "T"}),
        );
        rig
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn watcher(&mut self, watch: &[&str]) -> String {
        self.ok(
            "entity.spawn",
            json!({"components": {"InputActions": {"watch": watch}}}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn key(&mut self, tick: u64, key: &str, pressed: bool) {
        self.ok(
            "input.inject",
            json!({"events": [{"tick": tick, "event": {"Key": {"key": key, "pressed": pressed}}}]}),
        );
    }

    fn mirrored(&self, id: &str) -> InputActions {
        let id = id.parse().unwrap();
        self.engine
            .world(WorldSel::play)
            .unwrap()
            .get::<InputActions>(id)
            .cloned()
            .expect("InputActions in the play world")
    }
}

fn state(
    pressed: bool,
    just_pressed: bool,
    just_released: bool,
    value: ActionValue,
) -> ActionState {
    ActionState {
        pressed,
        just_pressed,
        just_released,
        value,
    }
}

fn button(on: bool) -> ActionValue {
    ActionValue::Button(on)
}

/// Space (`spawn_box`) and D (`camera_pan` right, so x = +1) pressed on tick 0, Space released on
/// tick 2. Tick 0: both just pressed. Tick 1: held. Tick 2: `spawn_box` just released with
/// value false; `camera_pan` still (1, 0). "fly" is unknown and `select` unwatched: absent.
#[test]
fn input_actions_component_mirrors_watched_actions() {
    let mut rig = Rig::new(vec![]);
    let id = rig.watcher(&["spawn_box", "camera_pan", "fly"]);
    rig.key(0, "Space", true);
    rig.key(0, "D", true);
    rig.key(2, "Space", false);
    let pan = ActionValue::Axis2d(Vec2::new(1.0, 0.0));

    rig.ok("sim.step", json!({"ticks": 1}));
    let m = rig.mirrored(&id);
    assert_eq!(
        m.states.keys().collect::<Vec<_>>(),
        ["camera_pan", "spawn_box"]
    );
    assert_eq!(
        m.states["spawn_box"],
        state(true, true, false, button(true))
    );
    assert_eq!(m.states["camera_pan"], state(true, true, false, pan));

    rig.ok("sim.step", json!({"ticks": 1}));
    let m = rig.mirrored(&id);
    assert_eq!(
        m.states["spawn_box"],
        state(true, false, false, button(true))
    );

    rig.ok("sim.step", json!({"ticks": 1}));
    let m = rig.mirrored(&id);
    assert_eq!(
        m.states["spawn_box"],
        state(false, false, true, button(false))
    );
    assert_eq!(m.states["camera_pan"], state(true, false, false, pan));
    assert_eq!(m.watch, ["spawn_box", "camera_pan", "fly"], "watch kept");
}

/// Space held from tick 0; the keyboard is taken on ticks 1 and 2. Tick 0: just pressed.
/// Tick 1: consumed, so released with `just_released` (the mirror showed it pressed). Tick 2:
/// still consumed, released without edges. Tick 3: seen again, so `just_pressed` though the
/// raw key never went up. The `ui.interact` stand-in runs before `input.actions` (its order).
#[test]
fn consumed_actions_read_as_released() {
    let taker = KeyTaker { ticks: vec![1, 2] };
    let mut rig = Rig::new(vec![Arc::new(taker)]);
    let id = rig.watcher(&["spawn_box"]);
    rig.key(0, "Space", true);
    let expected = [
        state(true, true, false, button(true)),
        state(false, false, true, button(false)),
        state(false, false, false, button(false)),
        state(true, true, false, button(true)),
    ];
    for (tick, want) in expected.iter().enumerate() {
        rig.ok("sim.step", json!({"ticks": 1}));
        assert_eq!(rig.mirrored(&id).states["spawn_box"], *want, "tick {tick}");
    }
}

/// The pure rule, case by case (see the module header).
#[test]
fn mirror_action_follows_the_rules() {
    let held = state(true, false, false, button(true));
    let released = state(false, false, false, button(false));
    // Consumed while shown pressed: released now.
    assert_eq!(
        mirror_action(Some(&held), &held, true),
        state(false, false, true, button(false))
    );
    // Consumed while already shown released: no edges.
    assert_eq!(mirror_action(Some(&released), &held, true), released);
    // Seen again while held: a new press.
    assert_eq!(
        mirror_action(Some(&released), &held, false),
        state(true, true, false, button(true))
    );
    // A tap inside one tick: both edges, released at the end.
    let tap = state(false, true, true, button(false));
    assert_eq!(mirror_action(None, &tap, false), tap);
    // A release the mirror never showed pressed (it was consumed): no edge.
    let let_go = state(false, false, true, button(false));
    assert_eq!(mirror_action(Some(&released), &let_go, false), released);
    // A consumed axis reads as its kind's zero.
    let axis = state(
        true,
        false,
        false,
        ActionValue::Axis2d(Vec2::new(0.0, -1.0)),
    );
    assert_eq!(
        mirror_action(None, &axis, true),
        state(false, false, false, ActionValue::Axis2d(Vec2::ZERO))
    );
}

/// The component is registered by the input module, so scenes, schemas and scripts name it.
#[test]
fn input_actions_is_a_registered_component() {
    let mut rig = Rig::new(vec![]);
    let schema = rig.ok("schema.get", json!({"name": "InputActions"}));
    assert!(schema.to_string().contains("watch"), "{schema}");
}
