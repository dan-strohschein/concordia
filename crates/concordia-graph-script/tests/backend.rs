//! Graphs as script modules (M4 4.10b, J1, J5–J7): compiled Event graphs run through the script
//! host on a real world, compiled or interpreted, and fault like scripts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use concordia_graph::{ExecLink, Graph, GraphKind, Input, Literal, Node, Signature, write_graph};
use concordia_graph_script::{GraphBackend, Mode};
use concordia_reflect::{Reflect, TypeRegistry};
use concordia_script::ModuleSource;
use concordia_script::conformance::Rig;
use concordia_script_wasm::{Fuel, WasmConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Hit points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "t.Health")]
struct Health {
    /// Current.
    value: f32,
    /// Most.
    max: f32,
}

/// Signal: took damage.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal, name = "t.Damaged")]
struct Damaged {
    /// How much.
    amount: f32,
}

fn registry() -> TypeRegistry {
    let mut reg = TypeRegistry::default();
    concordia_terra_sim::register(&mut reg).unwrap();
    concordia_terra::register_component::<Health>(&mut reg).unwrap();
    concordia_terra::register_component::<Damaged>(&mut reg).unwrap();
    reg
}

fn id(n: u32) -> String {
    format!("0193b0f2-0000-7000-8000-{n:012x}")
}

fn link(n: u32, pin: &str) -> Input {
    Input::Link(id(n), pin.into())
}

/// On tick: below max, regain 2/s (capped). The 4.9 `regenerate` example over `t.Health`.
fn regenerate() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../concordia-graph/tests/graphs/ok/regenerate.graph.ron");
    std::fs::read_to_string(path)
        .unwrap()
        .replace("Health", "t.Health")
}

/// Every tick, raise `t.Damaged { amount: 1 }` on itself; on `t.Damaged`, lose `amount`.
fn burn_and_suffer() -> String {
    let nodes = [
        (10, Node::new("event.on_tick")),
        (
            11,
            Node::new("raise.t.Damaged")
                .with_input("target", link(12, "entity"))
                .with_input("amount", Input::Value(Literal::Float(1.0))),
        ),
        (12, Node::new("entity.self")),
        (20, Node::new("on.t.Damaged")),
        (21, Node::new("get.t.Health.value")),
        (
            22,
            Node::new("math.sub")
                .with_input("a", link(21, "value"))
                .with_input("b", link(20, "amount")),
        ),
        (
            23,
            Node::new("set.t.Health.value").with_input("value", link(22, "out")),
        ),
    ];
    let graph = Graph {
        id: id(1),
        kind: GraphKind::Event,
        version: 1,
        signature: Signature::default(),
        variables: vec![],
        nodes: nodes.into_iter().map(|(n, node)| (id(n), node)).collect(),
        exec: vec![
            ExecLink(id(10), "out".into(), id(11), "in".into()),
            ExecLink(id(20), "out".into(), id(23), "in".into()),
        ],
        layout: BTreeMap::new(),
    };
    write_graph(&graph).unwrap()
}

fn rig(mode: Mode, name: &str, text: &str, threads: usize) -> Rig {
    let reg = registry();
    let module = GraphBackend::new(mode)
        .unwrap()
        .load_text(name, text, &reg)
        .unwrap();
    Rig::with_host_in(module, reg, threads, &[], |_| {})
}

fn value(rig: &Rig, e: concordia_core::id::EntityId) -> f64 {
    rig.get(e, "t.Health").unwrap()["value"].as_f64().unwrap()
}

#[test]
fn an_attached_graph_runs_every_tick() {
    let mut rig = rig(Mode::default(), "regenerate", &regenerate(), 1);
    let attached = rig.spawn(&[
        ("t.Health", json!({"value": 5.0, "max": 10.0})),
        ("graph.regenerate.Vars", json!({})),
    ]);
    let plain = rig.spawn(&[("t.Health", json!({"value": 5.0, "max": 10.0}))]);
    rig.step(30);
    assert!(
        (value(&rig, attached) - 6.0).abs() < 1e-4,
        "{}",
        value(&rig, attached)
    );
    assert_eq!(value(&rig, plain), 5.0, "graphs run only where attached");
    rig.step(600);
    assert_eq!(value(&rig, attached), 10.0, "capped at max");
    assert!(rig.host.faults().is_empty());
}

#[test]
fn raised_signals_reach_signal_processors() {
    let mut rig = rig(Mode::default(), "burn", &burn_and_suffer(), 1);
    let e = rig.spawn(&[
        ("t.Health", json!({"value": 100.0, "max": 100.0})),
        ("graph.burn.Vars", json!({})),
    ]);
    rig.step(10);
    // Each tick raises one; a raise is delivered the next tick.
    assert_eq!(value(&rig, e), 91.0);
    // Signals from outside reach it too.
    rig.signal(e, "t.Damaged", json!({"amount": 50.0}));
    rig.step(1);
    assert_eq!(value(&rig, e), 40.0);
}

#[test]
fn interpreted_and_compiled_graphs_agree() {
    let hash = |mode: Mode| {
        let mut rig = rig(mode, "burn", &burn_and_suffer(), 1);
        for k in 0..20 {
            rig.spawn(&[
                ("t.Health", json!({"value": 10.0 * k as f32, "max": 100.0})),
                ("graph.burn.Vars", json!({})),
            ]);
        }
        rig.step(15);
        rig.hash()
    };
    assert_eq!(
        hash(Mode::Interpret { budget: None }),
        hash(Mode::default())
    );
}

#[test]
fn compiled_graph_is_deterministic_across_runs_and_thread_counts() {
    let hash = |threads: usize| {
        let mut rig = rig(Mode::default(), "regenerate", &regenerate(), threads);
        for k in 0..200 {
            rig.spawn(&[
                ("t.Health", json!({"value": k as f32 * 0.05, "max": 10.0})),
                ("graph.regenerate.Vars", json!({})),
            ]);
        }
        rig.step(40);
        rig.hash()
    };
    let one = hash(1);
    assert_eq!(one, hash(1), "runs repeat");
    assert_eq!(one, hash(4), "thread count doesn't matter");
}

#[test]
fn runaway_graph_is_stopped_by_fuel_limit_without_crashing_engine() {
    let tiny = WasmConfig {
        fuel: Some(Fuel {
            per_run: 200,
            per_row: 0,
        }),
        ..WasmConfig::default()
    };
    let mut rig = rig(Mode::Wasm(tiny), "regenerate", &regenerate(), 1);
    for _ in 0..1000 {
        rig.spawn(&[
            ("t.Health", json!({"value": 1.0, "max": 10.0})),
            ("graph.regenerate.Vars", json!({})),
        ]);
    }
    rig.step(3);
    let faults = rig.host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(faults[0].fault.to_string().contains("fuel"), "{faults:?}");
    // The processor is off until reload; the world is untouched and keeps stepping.
    let some = rig.with(&["t.Health"])[0];
    assert_eq!(value(&rig, some), 1.0);
}

#[test]
fn a_broken_graph_fails_to_load_with_its_diagnostics() {
    let reg = registry();
    let broken = regenerate().replace("math.min", "math.minn");
    let err = GraphBackend::new(Mode::default())
        .unwrap()
        .load_text("regenerate", &broken, &reg)
        .err()
        .expect("a load error");
    let text = err.to_string();
    assert!(
        text.contains("unknown_op") && text.contains("math.min"),
        "{text}"
    );
}

#[test]
fn graph_files_load_by_path() {
    let reg = registry();
    let dir = std::env::temp_dir().join(format!("concordia-graph-script-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("regenerate.graph.ron");
    std::fs::write(&path, regenerate()).unwrap();
    let module = GraphBackend::new(Mode::default())
        .unwrap()
        .load(&ModuleSource::File(path), &reg)
        .unwrap();
    assert_eq!(module.describe().unwrap().name, "regenerate");
    std::fs::remove_dir_all(dir).ok();
}
