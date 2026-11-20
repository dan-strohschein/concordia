use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use proptest::prelude::*;

use super::*;
use crate::{Column, Command, SignalColumn, Target, Value};

fn ids(n: usize) -> Vec<EntityId> {
    let mut source = DeterministicIdGen::new(5);
    (0..n).map(|_| source.next_id().unwrap()).collect()
}

fn value(name: &str, data: &[u8]) -> Value {
    Value {
        name: name.into(),
        data: data.to_vec(),
    }
}

fn sample_call() -> (String, TickContext, Batch) {
    let ids = ids(3);
    let tick = TickContext {
        tick: 7,
        dt: 1.0 / 60.0,
        time: 0.125,
        seed: u64::MAX - 3,
    };
    let batch = Batch {
        ids: ids.clone(),
        columns: vec![
            Column {
                name: "m.Pos".into(),
                data: (0..24).collect(),
            },
            Column {
                name: "m.Empty".into(),
                data: Vec::new(),
            },
        ],
        signals: vec![SignalColumn {
            name: "m.Ping".into(),
            targets: ids[..2].to_vec(),
            data: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }],
    };
    ("m.move".into(), tick, batch)
}

fn every_command() -> Vec<Command> {
    let id = ids(1)[0];
    vec![
        Command::Spawn(vec![value("m.Pos", &[1, 2]), value("m.Tag", &[])]),
        Command::SpawnTemplate("crate".into()),
        Command::Insert(Target::Pending(1), value("m.Pos", &[9])),
        Command::Remove(Target::Entity(id), "m.Pos".into()),
        Command::Despawn(Target::Entity(id)),
        Command::Signal(Target::Pending(0), value("m.Ping", &[0; 4])),
    ]
}

fn sample_replies() -> Vec<Reply> {
    let output = RunOutput {
        writes: vec![Column {
            name: "m.Pos".into(),
            data: vec![3; 12],
        }],
        commands: every_command(),
    };
    vec![
        Reply {
            logs: vec!["one".into(), "twó".into()],
            outcome: Ok(output),
        },
        Reply {
            logs: Vec::new(),
            outcome: Ok(RunOutput::default()),
        },
        Reply {
            logs: vec!["before".into()],
            outcome: Err(GuestError::Panicked("boom".into())),
        },
        Reply {
            logs: Vec::new(),
            outcome: Err(GuestError::Returned("no".into())),
        },
        Reply {
            logs: Vec::new(),
            outcome: Err(GuestError::BadCall("short".into())),
        },
    ]
}

#[test]
fn calls_roundtrip_with_columns_copied_whole() {
    let (processor, tick, batch) = sample_call();
    let bytes = encode_call(&processor, &tick, &batch);
    let call = decode_call(&bytes).unwrap();
    assert_eq!(
        call,
        Call {
            processor,
            tick,
            batch
        }
    );
    let column = &call.batch.columns[0].data;
    let at = bytes
        .windows(column.len())
        .position(|w| w == column.as_slice());
    assert!(at.is_some(), "column bytes appear verbatim in the envelope");
}

#[test]
fn replies_roundtrip_for_every_command_and_error() {
    for reply in sample_replies() {
        assert_eq!(decode_reply(&encode_reply(&reply)).unwrap(), reply);
    }
}

#[test]
fn truncated_or_padded_envelopes_are_errors() {
    let (processor, tick, batch) = sample_call();
    let call = encode_call(&processor, &tick, &batch);
    let reply = encode_reply(&sample_replies()[0]);
    for bytes in [&call, &reply] {
        for cut in [0, 1, bytes.len() / 2, bytes.len() - 1] {
            assert!(decode_call(&bytes[..cut]).is_err() && decode_reply(&bytes[..cut]).is_err());
        }
    }
    let mut padded = call.clone();
    padded.push(0);
    assert!(decode_call(&padded).is_err(), "trailing bytes");
}

proptest! {
    #[test]
    fn decoding_never_panics_on_arbitrary_or_mutated_bytes(
        noise in proptest::collection::vec(any::<u8>(), 0..256),
        flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..8),
    ) {
        let _ = decode_call(&noise);
        let _ = decode_reply(&noise);
        let (processor, tick, batch) = sample_call();
        let mut call = encode_call(&processor, &tick, &batch);
        let mut reply = encode_reply(&sample_replies()[0]);
        prop_assert!(decode_call(&call).is_ok() && decode_reply(&reply).is_ok());
        for (at, byte) in flips {
            let i = at % call.len();
            call[i] ^= byte;
            let j = at % reply.len();
            reply[j] ^= byte;
        }
        let _ = decode_call(&call);
        let _ = decode_reply(&reply);
    }
}
