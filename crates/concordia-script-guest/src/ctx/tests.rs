use concordia_core::id::{DeterministicIdGen, IdSource};
use concordia_reflect::Reflect;
use concordia_reflect::wire::ColumnWriter;
use concordia_script_protocol::{Column, Command, SignalColumn};
use serde::{Deserialize, Serialize};

use super::*;

/// A test position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "sdk.Pos")]
struct Pos {
    /// X.
    x: f32,
    /// Name.
    tag: String,
}

fn ids(n: usize) -> Vec<EntityId> {
    let mut ids_gen = DeterministicIdGen::new(3);
    (0..n).map(|_| ids_gen.next_id().unwrap()).collect()
}

fn column(values: &[Pos]) -> Vec<u8> {
    let mut w = ColumnWriter::new(Pos::type_info(), values.len()).unwrap();
    for (row, v) in values.iter().enumerate() {
        w.write(row, v).unwrap();
    }
    w.finish().unwrap().into_bytes()
}

fn pos(x: f32) -> Pos {
    Pos {
        x,
        tag: format!("p{x}"),
    }
}

fn batch(values: &[Pos]) -> Batch {
    Batch {
        ids: ids(values.len()),
        columns: vec![Column {
            name: "sdk.Pos".into(),
            data: column(values),
        }],
        signals: Vec::new(),
    }
}

#[test]
fn reads_and_writes_whole_columns_on_the_wire() {
    let values = [pos(1.0), pos(2.0)];
    let batch = batch(&values);
    let mut sink = |_: &str| {};
    let mut ctx = RunCtx::new(TickContext::default(), &batch, &mut sink);
    let read: Vec<Pos> = ctx.read().unwrap();
    assert_eq!(read, values);
    let moved: Vec<Pos> = read.iter().map(|p| pos(p.x + 1.0)).collect();
    ctx.write(&moved).unwrap();
    let out = ctx.finish();
    assert_eq!(out.writes.len(), 1);
    assert_eq!(out.writes[0].name, "sdk.Pos");
    assert_eq!(out.writes[0].data, column(&moved));
}

#[test]
fn writing_the_wrong_row_count_or_an_undelivered_column_is_an_error() {
    let batch = batch(&[pos(1.0)]);
    let mut sink = |_: &str| {};
    let mut ctx = RunCtx::new(TickContext::default(), &batch, &mut sink);
    assert!(
        ctx.write(&[pos(1.0), pos(2.0)])
            .unwrap_err()
            .contains("rows")
    );
    let empty = Batch::default();
    let ctx = RunCtx::new(TickContext::default(), &empty, &mut sink);
    assert!(ctx.read::<Pos>().unwrap_err().contains("sdk.Pos"));
}

#[test]
fn commands_number_spawns_in_recording_order() {
    let batch = Batch::default();
    let mut sink = |_: &str| {};
    let mut ctx = RunCtx::new(TickContext::default(), &batch, &mut sink);
    let a = ctx.spawn(vec![value(&pos(0.0)).unwrap()]);
    let b = ctx.spawn_template("t");
    ctx.insert(b, &pos(5.0)).unwrap();
    ctx.remove::<Pos>(a);
    ctx.despawn(a);
    assert_eq!((a, b), (Target::Pending(0), Target::Pending(1)));
    let out = ctx.finish();
    let expected = vec![
        Command::Spawn(vec![value(&pos(0.0)).unwrap()]),
        Command::SpawnTemplate("t".into()),
        Command::Insert(b, value(&pos(5.0)).unwrap()),
        Command::Remove(a, "sdk.Pos".into()),
        Command::Despawn(a),
    ];
    assert_eq!(out.commands, expected);
}

#[test]
fn a_value_is_a_one_row_column() {
    let v = value(&pos(3.0)).unwrap();
    assert_eq!(v.name, "sdk.Pos");
    assert_eq!(v.data, column(&[pos(3.0)]));
}

#[test]
fn signals_decode_with_their_targets() {
    let targets = ids(2);
    let batch = Batch {
        signals: vec![SignalColumn {
            name: "sdk.Pos".into(),
            targets: targets.clone(),
            data: column(&[pos(1.0), pos(2.0)]),
        }],
        ..Batch::default()
    };
    let mut sink = |_: &str| {};
    let ctx = RunCtx::new(TickContext::default(), &batch, &mut sink);
    let got: Vec<(EntityId, Pos)> = ctx.signals().unwrap();
    assert_eq!(got, vec![(targets[0], pos(1.0)), (targets[1], pos(2.0))]);
}

#[test]
fn the_rng_is_the_engine_seeded_stream() {
    let batch = Batch::default();
    let mut sink = |_: &str| {};
    let tick = TickContext {
        seed: 99,
        ..TickContext::default()
    };
    let mut ctx = RunCtx::new(tick, &batch, &mut sink);
    assert_eq!(ctx.rng().next_u64(), SimRng::new(99).next_u64());
}

#[test]
fn log_lines_go_to_the_sink() {
    let batch = Batch::default();
    let mut lines = Vec::new();
    let mut sink = |l: &str| lines.push(l.to_owned());
    let mut ctx = RunCtx::new(TickContext::default(), &batch, &mut sink);
    ctx.log("hello");
    drop(ctx);
    assert_eq!(lines, ["hello"]);
}
