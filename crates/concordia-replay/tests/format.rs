//! The `.replay.ron` v0 format (M2 phase 2.8).

use std::collections::BTreeMap;

use concordia_api::Fingerprint;
use concordia_core::math::Vec2;
use concordia_input::{ActionState, ActionValue};
use concordia_replay::{
    Checkpoint, InputChange, Replay, ReplayError, ReplayHeader, read_replay, write_replay,
};

const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const H2: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn state(pressed: bool, value: ActionValue) -> ActionState {
    ActionState {
        pressed,
        just_pressed: pressed,
        just_released: false,
        value,
    }
}

fn checkpoint(tick: u64, world: &str) -> Checkpoint {
    Checkpoint {
        tick,
        world: world.into(),
        parts: BTreeMap::from([
            ("@clock".to_owned(), H2.to_owned()),
            ("Transform".to_owned(), H1.to_owned()),
        ]),
    }
}

fn sample() -> Replay {
    Replay {
        version: 0,
        header: ReplayHeader {
            engine: "0.0.0".into(),
            scene: "scenes/test.scene.ron".into(),
            scene_hash: H1.into(),
            seed: 5,
            rate_hz: 60,
            threads: 8,
            checkpoint_every: 60,
            modules: vec![Fingerprint {
                module: "scripts".into(),
                name: "wanderers".into(),
                kind: "wasm".into(),
                hash: H2.into(),
            }],
            recorded_by: "agent:qa".into(),
        },
        ticks: 120,
        inputs: vec![
            InputChange {
                tick: 10,
                actions: BTreeMap::from([
                    (
                        "camera_pan".to_owned(),
                        state(true, ActionValue::Axis2d(Vec2::new(0.0, 1.0))),
                    ),
                    ("select".to_owned(), state(true, ActionValue::Button(true))),
                ]),
            },
            InputChange {
                tick: 11,
                actions: BTreeMap::from([(
                    "select".to_owned(),
                    state(false, ActionValue::Button(false)),
                )]),
            },
        ],
        checkpoints: vec![checkpoint(0, H1), checkpoint(60, H2), checkpoint(120, H1)],
    }
}

#[test]
fn replay_file_is_canonical_ron_v0() {
    let text = write_replay(&sample()).unwrap();
    insta::assert_snapshot!(text);
    let read = read_replay(&text).unwrap();
    assert_eq!(read, sample());
    assert_eq!(
        write_replay(&read).unwrap(),
        text,
        "byte-identical round trip"
    );
}

#[test]
fn replay_format_rejects_other_versions_bad_hashes_and_unordered_ticks() {
    let text = |replay: &Replay| write_replay(replay).unwrap();

    let mut v1 = sample();
    v1.version = 1;
    let err = read_replay(&text(&v1)).unwrap_err();
    assert!(matches!(err, ReplayError::Version { found: 1 }), "{err:?}");

    let mut short = sample();
    short.checkpoints[1]
        .parts
        .insert("Velocity".into(), "abc".into());
    let err = read_replay(&text(&short)).unwrap_err();
    match err {
        ReplayError::Hash { at, value } => {
            assert_eq!(
                (at.as_str(), value.as_str()),
                ("checkpoints[1].parts.Velocity", "abc")
            );
        }
        other => panic!("{other:?}"),
    }

    let mut unordered = sample();
    unordered.inputs.swap(0, 1);
    let err = read_replay(&text(&unordered)).unwrap_err();
    assert!(
        matches!(err, ReplayError::Order { tick: 10, .. }),
        "{err:?}"
    );

    let mut past_end = sample();
    past_end.checkpoints[2].tick = 121;
    let err = read_replay(&text(&past_end)).unwrap_err();
    assert!(
        matches!(err, ReplayError::Order { tick: 121, .. }),
        "{err:?}"
    );

    let err = read_replay("Replay(version: 0)").unwrap_err();
    assert!(matches!(err, ReplayError::Parse(_)), "{err:?}");
}
