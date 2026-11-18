//! `input.inject` through the Engine API (M1 phase 1.1).

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig, WorldSel, params_schema};
use concordia_core::id::SessionId;
use concordia_core::log::LogRing;
use concordia_input::{ActionStates, InputMap, InputModule};
use serde_json::{Value, json};
use tracing_subscriber::layer::SubscriberExt;

const DEFAULT: &str = include_str!("../../../examples/m1-physics-stack/input/default.input.ron");

struct Rig {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    fn new(ring: Option<Arc<LogRing>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(7);
        config.threads = NonZeroUsize::MIN;
        config.journal.sync = false;
        config.log_ring = ring;
        let module = InputModule::new(InputMap::parse(DEFAULT).unwrap());
        config.modules = vec![Arc::new(module)];
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

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn states(&self) -> ActionStates {
        self.engine
            .world(WorldSel::play)
            .unwrap()
            .resource::<ActionStates>()
            .cloned()
            .unwrap_or_default()
    }
}

fn click(tick: Option<u64>) -> Value {
    json!({"tick": tick, "event": {"MouseButton": {"button": "Left", "pressed": true}}})
}

#[test]
fn input_inject_schema_is_generated_from_reflect() {
    let rig = Rig::new(None);
    let method = rig
        .engine
        .methods()
        .get("input.inject")
        .expect("registered");
    assert!(!method.mutates, "input acts on the play world only");
    insta::assert_json_snapshot!(params_schema(method));
}

#[test]
fn input_inject_creates_paused_play_world() {
    let mut rig = Rig::new(None);
    assert!(rig.engine.world(WorldSel::play).is_err());
    let result = rig.ok("input.inject", json!({"events": [click(None)]}));
    assert_eq!(result["accepted"], 1);
    assert_eq!(rig.ok("sim.pause", json!({}))["state"], "paused");
    rig.ok("sim.step", json!({"ticks": 1}));
    assert!(rig.states().get("select").unwrap().just_pressed);
}

#[test]
fn inject_at_tick_applies_then_and_not_before() {
    let mut rig = Rig::new(None);
    rig.ok("input.inject", json!({"events": [click(Some(10))]}));
    rig.ok("sim.step", json!({"ticks": 10}));
    assert!(!rig.states().get("select").unwrap().pressed, "ticks 0..=9");
    rig.ok("sim.step", json!({"ticks": 1}));
    assert!(rig.states().get("select").unwrap().just_pressed, "tick 10");
}

#[test]
fn input_inject_rejects_unknown_action_with_invalid_params() {
    let mut rig = Rig::new(None);
    let event = json!({"Action": {"action": "fly", "value": {"Button": true}}});
    let err = rig
        .call("input.inject", json!({"events": [{"event": event}]}))
        .unwrap_err();
    assert_eq!(err.code(), -32602, "{err:?}");
    let known = err.data.extra.get("known_actions").expect("known_actions");
    assert!(known.to_string().contains("camera_pan"), "{known}");
}

#[test]
fn input_inject_rejects_past_ticks_and_applies_nothing() {
    let mut rig = Rig::new(None);
    rig.ok("sim.step", json!({"ticks": 5}));
    let err = rig
        .call(
            "input.inject",
            json!({"events": [click(Some(9)), click(Some(2))]}),
        )
        .unwrap_err();
    assert_eq!(err.code(), -32602, "{err:?}");
    rig.ok("sim.step", json!({"ticks": 5}));
    assert!(
        !rig.states().get("select").unwrap().pressed,
        "nothing applied"
    );
}

#[test]
fn inject_is_attributed_in_log_tail_and_not_journaled() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(64).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let mut rig = Rig::new(Some(Arc::clone(&ring)));
        let before = rig.ok("txn.history", json!({}));
        rig.ok(
            "input.inject",
            json!({"events": [click(None)], "actor": "agent:qa#2"}),
        );
        let records = rig.ok("log.tail", json!({"level": "info"}))["records"].clone();
        let injected = records
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["message"] == "input injected")
            .unwrap_or_else(|| panic!("no 'input injected' record: {records}"));
        assert_eq!(injected["fields"]["actor"], "agent:qa#2");
        assert_eq!(injected["fields"]["events"], "1");
        assert_eq!(rig.ok("txn.history", json!({})), before);
    });
}
