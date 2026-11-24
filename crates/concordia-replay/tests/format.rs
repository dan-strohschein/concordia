//! The `.replay.ron` format: version 1 (M3 phase 3.1a) and reading version 0 (M2 phase 2.8).
//!
//! The sample's per-tick hashes are each tick's number as 16 hex digits, so tick t's entry is
//! easy to read off: 120 ticks make one full line of 64 (ticks 1..=64) and one of 56.

use std::collections::BTreeMap;

use concordia_api::Fingerprint;
use concordia_core::math::Vec2;
use concordia_input::{ActionState, ActionValue};
use concordia_replay::{
    Checkpoint, InputChange, Replay, ReplayError, ReplayHeader, TICKS_PER_LINE, read_replay,
    write_replay,
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

/// Per-tick hashes for ticks 1..=`ticks`: each tick's number in 16 hex digits.
fn tick_hashes(ticks: u64) -> Vec<String> {
    let all: Vec<String> = (1..=ticks).map(|t| format!("{t:016x}")).collect();
    all.chunks(TICKS_PER_LINE).map(|c| c.concat()).collect()
}

fn sample() -> Replay {
    Replay {
        version: 1,
        header: ReplayHeader {
            engine: "0.0.0".into(),
            platform: Some("macos-aarch64".into()),
            manifest_hash: Some(H2.into()),
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
            task: Some("T-0001".into()),
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
                text: Vec::new(),
            },
            InputChange {
                tick: 11,
                actions: BTreeMap::from([(
                    "select".to_owned(),
                    state(false, ActionValue::Button(false)),
                )]),
                text: Vec::new(),
            },
        ],
        checkpoints: vec![checkpoint(0, H1), checkpoint(60, H2), checkpoint(120, H1)],
        hashes: tick_hashes(120),
    }
}

#[test]
fn replay_ron_roundtrip_is_byte_identical() {
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
fn replay_format_rejects_unknown_major_version_bad_hashes_and_unordered_ticks() {
    let text = |replay: &Replay| write_replay(replay).unwrap();

    let mut v2 = sample();
    v2.version = 2;
    let err = read_replay(&text(&v2)).unwrap_err();
    assert!(matches!(err, ReplayError::Version { found: 2 }), "{err:?}");

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

    let err = read_replay("Replay(version: 1)").unwrap_err();
    assert!(matches!(err, ReplayError::Parse(_)), "{err:?}");
}

/// Typed text (2.7b.3) is stored on its tick, beside or without action changes, and a tick with
/// none writes no `text` field.
#[test]
fn typed_text_round_trips_and_is_omitted_when_empty() {
    use concordia_input::{EditKey, TextInput};
    let mut replay = sample();
    replay.inputs.push(InputChange {
        tick: 12,
        actions: BTreeMap::new(),
        text: vec![
            TextInput::Insert("hi".into()),
            TextInput::Edit {
                key: EditKey::Backspace,
                select: false,
            },
        ],
    });
    let text = write_replay(&replay).unwrap();
    assert_eq!(text.matches("text:").count(), 1, "{text}");
    assert!(text.contains("Insert(\"hi\")"), "{text}");
    assert_eq!(read_replay(&text).unwrap(), replay);
    assert!(!write_replay(&sample()).unwrap().contains("text:"));
}

/// Every tick has a hash: 120 ticks need one full line (64 × 16 = 1,024 hex digits) and one of
/// 56 × 16 = 896. A short first line, a missing tick, an extra one or a non-hex digit is refused,
/// naming the line.
#[test]
fn tick_hashes_must_cover_every_tick_in_full_lines() {
    let replay = sample();
    assert_eq!(replay.hashes.len(), 2);
    assert_eq!(
        (replay.hashes[0].len(), replay.hashes[1].len()),
        (1024, 896)
    );
    assert_eq!(replay.tick_hash(1), Some("0000000000000001"));
    assert_eq!(replay.tick_hash(65), Some("0000000000000041"));
    assert_eq!(replay.tick_hash(120), Some("0000000000000078"));
    assert_eq!((replay.tick_hash(0), replay.tick_hash(121)), (None, None));
    let bad = |edit: fn(&mut Vec<String>)| {
        let mut r = sample();
        edit(&mut r.hashes);
        read_replay(&write_replay(&r).unwrap()).unwrap_err()
    };
    for (err, line) in [
        (bad(|h| h[0].truncate(1008)), 0),
        (bad(|h| h[1].truncate(880)), 1),
        (bad(|h| h[1].push_str("0000000000000079")), 1),
        (bad(|h| h[0].replace_range(0..1, "g")), 0),
    ] {
        match err {
            ReplayError::TickHashes { line: l, .. } => assert_eq!(l, line, "{err:?}"),
            other => panic!("{other:?}"),
        }
    }
}

/// A version 0 file (M2 2.8, the old canonical sample) reads as version 1: the same header,
/// inputs and checkpoints, with no platform, manifest hash, task or per-tick hashes (those were
/// never recorded, so runs don't compare them).
#[test]
fn a_version_0_file_reads_as_version_1() {
    let text = include_str!("fixtures/v0.replay.ron");
    assert!(text.contains("version: 0"));
    let replay = read_replay(text).unwrap();
    assert_eq!(replay.version, 1);
    let mut want = sample();
    want.header.platform = None;
    want.header.manifest_hash = None;
    want.header.task = None;
    want.hashes = Vec::new();
    assert_eq!(replay, want);
    let again = write_replay(&replay).unwrap();
    assert!(
        again.contains("version: 1") && !again.contains("hashes:"),
        "{again}"
    );
}

proptest::proptest! {
    /// Any input stream (strictly increasing ticks, button and axis states, typed text) writes
    /// and reads back unchanged.
    #[test]
    fn prop_input_stream_roundtrips(
        steps in proptest::collection::vec(
            (1u64..50, proptest::bool::ANY, -1.0f32..1.0, -1.0f32..1.0, "[a-z ]{0,6}"),
            0..20,
        )
    ) {
        use concordia_input::TextInput;
        let mut replay = sample();
        let mut tick = 0;
        replay.inputs = steps
            .into_iter()
            .map(|(gap, pressed, x, y, typed)| {
                tick += gap;
                InputChange {
                    tick,
                    actions: BTreeMap::from([
                        ("select".to_owned(), state(pressed, ActionValue::Button(pressed))),
                        ("pan".to_owned(), state(pressed, ActionValue::Axis2d(Vec2::new(x, y)))),
                    ]),
                    text: if typed.is_empty() { Vec::new() } else { vec![TextInput::Insert(typed)] },
                }
            })
            .collect();
        replay.ticks = tick.max(1);
        replay.checkpoints = vec![checkpoint(0, H1), checkpoint(replay.ticks, H2)];
        replay.hashes = tick_hashes(replay.ticks);
        let text = write_replay(&replay).unwrap();
        proptest::prop_assert_eq!(read_replay(&text).unwrap(), replay);
    }
}
