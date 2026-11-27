//! Event graphs as processor programs (M4 4.10b, J13–J15): predicated bodies over wire columns,
//! run by the interpreter on the same memory image the compiled module sees.

use std::path::Path;

use concordia_core::math::Vec3;
use concordia_graph::runtime::{Entry, Image};
use concordia_graph::{Code, Graph, NodeLibrary, compile_event, read_graph};
use concordia_reflect::{Reflect, TypeRegistry, wire};
use serde::{Deserialize, Serialize};

/// Hit points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
struct Health {
    /// Current.
    value: f32,
    /// Most.
    max: f32,
}

/// Movement.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
struct Mover {
    /// Metres per second.
    velocity: Vec3,
}

/// Signal: took damage.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal)]
struct Damaged {
    /// How much.
    amount: f32,
}

fn registry() -> TypeRegistry {
    let mut reg = TypeRegistry::new();
    reg.register::<Health>().unwrap();
    reg.register::<Mover>().unwrap();
    reg.register::<Damaged>().unwrap();
    reg
}

fn library(reg: &TypeRegistry) -> NodeLibrary {
    NodeLibrary::build(reg, |r| {
        ["Health", "Mover", "Damaged"].contains(&r.info().name)
    })
}

fn example(name: &str) -> Graph {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/graphs/ok/{name}.graph.ron"));
    read_graph(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn column<T: Reflect + Serialize>(values: &[T]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|v| wire::encode(v, T::type_info()).unwrap())
        .collect()
}

fn rows<T: Reflect + serde::de::DeserializeOwned>(bytes: &[u8]) -> Vec<T> {
    let stride = wire::layout(T::type_info()).unwrap().stride();
    bytes
        .chunks(stride)
        .map(|c| wire::decode(c, T::type_info()).unwrap())
        .collect()
}

/// Every column slot's bytes for `rows` matched entities: the given ones, zeros for `Vars`.
fn slots(
    program: &concordia_graph::runtime::Program,
    given: &[(&str, Vec<u8>)],
    rows: usize,
) -> Vec<Vec<u8>> {
    program
        .columns()
        .iter()
        .map(|slot| {
            given
                .iter()
                .find(|(n, _)| *n == slot.name)
                .map_or_else(|| vec![0; slot.stride * rows], |(_, b)| b.clone())
        })
        .collect()
}

#[test]
fn a_tick_graph_updates_every_row_in_place() {
    let reg = registry();
    let module = compile_event("regenerate", &example("regenerate"), &library(&reg), &reg).unwrap();
    assert_eq!(module.processors.len(), 1);
    let p = &module.processors[0];
    assert_eq!(p.entry, Entry::Tick);
    assert!(p.writes.contains(&"Health".to_owned()), "{:?}", p.writes);
    // Below max: +2/s × 0.5 s, capped at max; at max: unchanged.
    let health = column(&[
        Health {
            value: 5.0,
            max: 10.0,
        },
        Health {
            value: 9.5,
            max: 10.0,
        },
        Health {
            value: 10.0,
            max: 10.0,
        },
    ]);
    let mut image = Image::tick(p, 3, 0.5, &slots(p, &[("Health", health)], 3)).unwrap();
    p.interpret(&mut image).unwrap();
    let after: Vec<Health> = rows(image.column(p, "Health").unwrap());
    let values: Vec<f32> = after.iter().map(|h| h.value).collect();
    assert_eq!(values, [6.0, 10.0, 10.0]);
    assert_eq!(after[0].max, 10.0, "untouched fields keep their bytes");
}

#[test]
fn signals_apply_in_order_to_their_targets() {
    let reg = registry();
    let module =
        compile_event("take_damage", &example("take_damage"), &library(&reg), &reg).unwrap();
    let p = &module.processors[0];
    assert_eq!(p.entry, Entry::Signal("Damaged".into()));
    let health = column(&[
        Health {
            value: 10.0,
            max: 10.0,
        },
        Health {
            value: 3.0,
            max: 10.0,
        },
    ]);
    // Row 1 takes 2 then 2 (3 → 1 → 0, floored); a signal for an unmatched entity is skipped.
    let payload = column(&[
        Damaged { amount: 2.0 },
        Damaged { amount: 2.0 },
        Damaged { amount: 50.0 },
    ]);
    let mut image = Image::signals(
        p,
        &[Some(1), Some(1), None],
        &payload,
        &slots(p, &[("Health", health)], 2),
    )
    .unwrap();
    p.interpret(&mut image).unwrap();
    let values: Vec<f32> = rows::<Health>(image.column(p, "Health").unwrap())
        .iter()
        .map(|h| h.value)
        .collect();
    assert_eq!(values, [10.0, 0.0]);
}

#[test]
fn raised_signals_come_out_as_wire_payloads() {
    let reg = registry();
    let module = compile_event("burn", &example("burn"), &library(&reg), &reg).unwrap();
    let p = &module.processors[0];
    let health = column(&[
        Health {
            value: 3.0,
            max: 10.0,
        },
        Health {
            value: 0.0,
            max: 10.0,
        },
    ]);
    let mut image = Image::tick(p, 2, 1.0 / 60.0, &slots(p, &[("Health", health)], 2)).unwrap();
    p.interpret(&mut image).unwrap();
    let raises = image.raises(p).unwrap();
    assert_eq!(raises.len(), 1, "only the living entity burns");
    assert_eq!((raises[0].job, raises[0].signal.as_str()), (0, "Damaged"));
    let d: Damaged = wire::decode(&raises[0].payload, Damaged::type_info()).unwrap();
    assert_eq!(d.amount, 1.5);
}

#[test]
fn variables_live_in_the_vars_component() {
    let reg = registry();
    let module = compile_event(
        "tick_counter",
        &example("tick_counter"),
        &library(&reg),
        &reg,
    )
    .unwrap();
    assert_eq!(module.vars.name, "graph.tick_counter.Vars");
    let p = &module.processors[0];
    assert!(p.writes.contains(&module.vars.name));
    // ticks: 0 → 1; 59 → 60, which resets to 0 (sequence: add first, then the check).
    let vars = [0i32, 59]
        .iter()
        .flat_map(|t| t.to_le_bytes())
        .collect::<Vec<u8>>();
    let mut image = Image::tick(
        p,
        2,
        1.0 / 60.0,
        &slots(p, &[(module.vars.name.as_str(), vars)], 2),
    )
    .unwrap();
    p.interpret(&mut image).unwrap();
    let out = image.column(p, &module.vars.name).unwrap();
    let ticks: Vec<i32> = out
        .chunks(4)
        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(ticks, [1, 0]);
}

#[test]
fn vector_fields_round_trip_through_components() {
    let reg = registry();
    let module = compile_event("steer", &example("steer"), &library(&reg), &reg).unwrap();
    let p = &module.processors[0];
    let mover = column(&[Mover {
        velocity: Vec3::new(0.0, 0.0, 0.0),
    }]);
    let mut image = Image::tick(p, 1, 0.5, &slots(p, &[("Mover", mover)], 1)).unwrap();
    p.interpret(&mut image).unwrap();
    let after: Vec<Mover> = rows(image.column(p, "Mover").unwrap());
    assert_eq!(after[0].velocity, Vec3::new(0.0, 0.0, 2.0));
}

#[test]
fn unsupported_event_nodes_are_located() {
    let reg = registry();
    let lib = library(&reg);
    // query.each reaches other entities.
    let errors = compile_event("heal_everyone", &example("heal_everyone"), &lib, &reg).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.code == Code::unsupported_node && d.node_id.is_some()),
        "{errors:?}"
    );
    // An exec loop can't be predicated.
    let mut looping = example("regenerate");
    let (from, to) = (looping.exec[1].2.clone(), looping.exec[0].2.clone());
    looping.exec.push(concordia_graph::ExecLink(
        from,
        "out".into(),
        to,
        "in".into(),
    ));
    let errors = compile_event("looping", &looping, &lib, &reg).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.code == Code::unsupported_node && d.message.contains("loop")),
        "{errors:?}"
    );
}

/// Runs `wasm`'s `run` on a copy of `image` in its own memory and returns the memory's image part.
fn run_wasm(wasm: &[u8], image: &Image) -> Vec<u8> {
    let mut config = wasmtime::Config::new();
    config.cranelift_nan_canonicalization(true);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let module = wasmtime::Module::new(&engine, wasm).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let memory = instance
        .get_memory(&mut store, "memory")
        .expect("a `memory` export");
    let pages = image.bytes.len().div_ceil(65536) as u64;
    let have = memory.size(&store);
    if pages > have {
        memory.grow(&mut store, pages - have).unwrap();
    }
    memory.write(&mut store, 0, &image.bytes).unwrap();
    let run = instance
        .get_typed_func::<(), ()>(&mut store, "run")
        .unwrap();
    run.call(&mut store, ()).unwrap();
    memory.data(&store)[..image.bytes.len()].to_vec()
}

fn f32s() -> impl proptest::strategy::Strategy<Value = f32> {
    use proptest::prelude::*;
    prop_oneof![6 => -100f32..100f32, 1 => Just(0.0f32), 1 => Just(-0.0f32), 1 => Just(f32::NAN), 1 => Just(f32::INFINITY)]
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig { cases: 64, ..Default::default() })]

    #[test]
    fn interpreted_and_compiled_event_graphs_agree(
        values in proptest::collection::vec(f32s(), 12),
        counts in proptest::collection::vec(-100i32..100, 4),
        dt in 0.0f32..0.1,
    ) {
        let reg = registry();
        let lib = library(&reg);
        let health = column(&[0, 1, 2, 3].map(|k| Health { value: values[2 * k], max: values[2 * k + 1] }));
        let mover = column(&[0, 1].map(|k| Mover { velocity: Vec3::new(values[8 + k], values[9 + k], values[10]) }));
        let ticks: Vec<u8> = counts.iter().flat_map(|t| t.to_le_bytes()).collect();
        for name in ["regenerate", "take_damage", "burn", "tick_counter", "steer"] {
            let module = compile_event(name, &example(name), &lib, &reg).unwrap();
            let vars = module.vars.name.clone();
            for p in &module.processors {
                let (n, given) = match name {
                    "steer" => (2, vec![("Mover", mover.clone())]),
                    "tick_counter" => (4, vec![(vars.as_str(), ticks.clone())]),
                    _ => (4, vec![("Health", health.clone())]),
                };
                let columns = slots(p, &given, n);
                let image = match &p.entry {
                    Entry::Tick => Image::tick(p, n, dt, &columns).unwrap(),
                    Entry::Signal(_) => {
                        let payload = column(&[Damaged { amount: values[0] }, Damaged { amount: values[1] }, Damaged { amount: values[2] }]);
                        Image::signals(p, &[Some(1), Some(1), Some(3)], &payload, &columns).unwrap()
                    }
                };
                let mut interpreted = image.clone();
                p.interpret(&mut interpreted).unwrap();
                let compiled = run_wasm(&p.wasm(), &image);
                proptest::prop_assert_eq!(&interpreted.bytes, &compiled, "{}", p.name);
            }
        }
    }
}

#[test]
fn compiled_programs_validate() {
    let reg = registry();
    let lib = library(&reg);
    for name in ["regenerate", "take_damage", "burn", "tick_counter", "steer"] {
        for p in compile_event(name, &example(name), &lib, &reg)
            .unwrap()
            .processors
        {
            wasmparser::Validator::new()
                .validate_all(&p.wasm())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}

#[test]
fn graph_trace_reports_executed_nodes_in_order() {
    let reg = registry();
    let graph = example("regenerate");
    let module = compile_event("regenerate", &graph, &library(&reg), &reg).unwrap();
    let p = &module.processors[0];
    let op = |id: &String| graph.nodes[id].op.clone();
    let health = column(&[
        Health {
            value: 5.0,
            max: 10.0,
        },
        Health {
            value: 10.0,
            max: 10.0,
        },
    ]);
    let image = Image::tick(p, 2, 0.5, &slots(p, &[("Health", health)], 2)).unwrap();
    let below = p.trace_job(&image, 0).unwrap();
    assert_eq!(
        below.executed.iter().map(op).collect::<Vec<_>>(),
        ["branch", "set.Health.value"]
    );
    let at_max = p.trace_job(&image, 1).unwrap();
    assert_eq!(
        at_max.executed.iter().map(op).collect::<Vec<_>>(),
        ["branch"]
    );
    let read = |t: &concordia_graph::runtime::JobTrace, wanted: &str| {
        t.pins
            .iter()
            .find(|pin| op(&pin.node) == wanted)
            .map(|pin| pin.values.clone())
    };
    assert_eq!(read(&below, "get.Health.value"), Some(vec![5.0]));
    assert_eq!(
        read(&below, "math.min"),
        Some(vec![6.0]),
        "the value it would set"
    );
    assert_eq!(read(&at_max, "math.lt"), Some(vec![0.0]));
    // Tracing is read only.
    assert_eq!(
        image,
        Image::tick(
            p,
            2,
            0.5,
            &slots(
                p,
                &[(
                    "Health",
                    column(&[
                        Health {
                            value: 5.0,
                            max: 10.0
                        },
                        Health {
                            value: 10.0,
                            max: 10.0
                        }
                    ])
                )],
                2
            )
        )
        .unwrap()
    );
}
