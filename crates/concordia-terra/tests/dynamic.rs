//! Dynamic components and named access (M2 phase 2.6a, ADR-0062): types that script modules
//! describe at runtime are stored, queried, hashed and scheduled exactly like Rust components,
//! and processors can reach any component by stable name through wire batches.

mod common;

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use common::{Pos, Vel, ids, registry};
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_reflect::wire::ColumnWriter;
use concordia_reflect::{DynReflect, DynamicValue, Reflect, TypeDesc, TypeInfo, TypeRegistry};
use concordia_terra::{
    DynamicQuery, NamedQuery, Phase, QueryError, ReflectComponent, Schedule, SimLoop, StorageKind,
    SystemCtx, View, World, WorldHash, processor, register_dynamic_component,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A static signal, received by name in one test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Hit(pub u32);

fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

fn desc(name: &str, shape: Value) -> TypeDesc {
    serde_json::from_value(
        json!({"name": name, "docs": format!("{name} (script)."), "shape": shape}),
    )
    .expect("valid TypeDesc")
}

fn field(name: &str, ty: &str) -> Value {
    json!({"name": name, "ty": {"Named": ty}, "docs": name})
}

/// `Brain { mood: f32, label: String }`, `Calm` (a tag) and `Ping { n: u32 }`.
fn script_registry() -> (TypeRegistry, &'static TypeInfo, &'static TypeInfo) {
    let mut reg = registry();
    let brain = register_dynamic_component(
        &mut reg,
        &desc(
            "Brain",
            json!({"Struct": {"fields": [field("mood", "f32"), field("label", "String")], "deny_unknown": true}}),
        ),
    )
    .unwrap();
    register_dynamic_component(&mut reg, &desc("Calm", json!("Unit"))).unwrap();
    let ping = register_dynamic_component(
        &mut reg,
        &desc(
            "Ping",
            json!({"Struct": {"fields": [field("n", "u32")], "deny_unknown": true}}),
        ),
    )
    .unwrap();
    concordia_terra::register_component::<Hit>(&mut reg).unwrap();
    (reg, brain, ping)
}

fn component(reg: &TypeRegistry, name: &str) -> ReflectComponent {
    *reg.get_by_name(name)
        .and_then(|r| r.data::<ReflectComponent>())
        .expect("a registered component")
}

fn brain(info: &'static TypeInfo, mood: f32, label: &str) -> Box<dyn DynReflect> {
    Box::new(DynamicValue::from_json(info, json!({"mood": mood, "label": label})).unwrap())
}

/// `n` entities with `Pos { x: i }`, the first `with_brain` of them also with a `Brain`.
fn script_world(reg: &TypeRegistry, n: usize, with_brain: usize) -> (World, Vec<EntityId>) {
    let mut world = World::new();
    let ids = ids(n);
    for (i, &id) in ids.iter().enumerate() {
        world.spawn_with_id(id).unwrap();
        world
            .insert(
                id,
                Pos {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            )
            .unwrap();
        if i < with_brain {
            world
                .set_json(id, "Brain", json!({"mood": 0.0, "label": "idle"}), reg)
                .unwrap();
        }
    }
    (world, ids)
}

#[test]
fn dynamic_component_stores_queries_and_hashes_by_name() {
    let (reg, _, _) = script_registry();
    let (mut world, ids) = script_world(&reg, 3, 0);
    let a = ids[0];
    let before = world.state_hash().unwrap();
    world
        .set_json(a, "Brain", json!({"mood": 0.5, "label": "calm"}), &reg)
        .unwrap();
    assert_eq!(
        world.get_json(a, "Brain").unwrap(),
        json!({"mood": 0.5, "label": "calm"})
    );
    assert_eq!(world.component_names(a).unwrap(), vec!["Brain", "Pos"]);
    let query = DynamicQuery {
        with: vec!["Brain".into()],
        without: vec![],
    };
    assert_eq!(query.run(&world, &reg).unwrap(), vec![a]);

    let with_brain = world.state_hash().unwrap();
    assert_ne!(with_brain, before);
    world
        .set_json(a, "Brain", json!({"mood": 0.75, "label": "calm"}), &reg)
        .unwrap();
    assert_ne!(world.state_hash().unwrap(), with_brain);
    let play = world.clone();
    assert_eq!(play.get_json(a, "Brain").unwrap()["mood"], json!(0.75));
    world
        .set_json(a, "Brain", json!({"mood": 0.5, "label": "calm"}), &reg)
        .unwrap();
    assert_eq!(world.state_hash().unwrap(), with_brain);

    assert!(
        world
            .set_json(a, "Brain", json!({"mood": "sad", "label": "x"}), &reg)
            .is_err()
    );
    let removed = world.remove_dyn(a, "Brain").unwrap().expect("had a Brain");
    assert_eq!(removed.info().name, "Brain");
    assert_eq!(world.state_hash().unwrap(), before);
    world.validate_storage().unwrap();
}

#[test]
fn dynamic_component_hashes_like_the_equivalent_static_type() {
    let mut twin = TypeRegistry::new();
    let mut pos_desc = Pos::type_info().to_desc();
    pos_desc.docs = "A script's own Pos.".into();
    register_dynamic_component(&mut twin, &pos_desc).unwrap();

    let ids = ids(4);
    let mut typed = World::new();
    let mut dynamic = World::new();
    for (i, &id) in ids.iter().enumerate() {
        let value = Pos {
            x: i as f32,
            y: -1.5,
            z: 1e-3,
        };
        typed.spawn_with_id(id).unwrap();
        typed.insert(id, value).unwrap();
        dynamic.spawn_with_id(id).unwrap();
        dynamic
            .set_json(id, "Pos", serde_json::to_value(value).unwrap(), &twin)
            .unwrap();
    }
    assert_eq!(typed.state_hash().unwrap(), dynamic.state_hash().unwrap());
}

#[test]
fn dynamic_and_static_components_share_an_archetype() {
    let (reg, brain_info, _) = script_registry();
    let (mut world, ids) = script_world(&reg, 2, 2);
    for &id in &ids {
        world.set_json(id, "Calm", Value::Null, &reg).unwrap();
    }
    let a = world.entity_storage(ids[0]).unwrap();
    let b = world.entity_storage(ids[1]).unwrap();
    assert_eq!(a.archetype, b.archetype);
    assert_eq!(
        a.components,
        vec![
            ("Brain", StorageKind::Dense),
            ("Calm", StorageKind::Tag),
            ("Pos", StorageKind::Dense),
        ]
    );
    world
        .insert_dyn(ids[0], brain(brain_info, 2.0, "x"), &reg)
        .unwrap();
    assert_eq!(world.get_json(ids[0], "Brain").unwrap()["mood"], json!(2.0));
    assert_eq!(
        world.entity_storage(ids[0]).unwrap().archetype,
        b.archetype,
        "replacing a value never moves the entity"
    );
    world.validate_storage().unwrap();
}

#[test]
fn named_access_conflicts_like_typed_access() {
    let (reg, _, _) = script_registry();
    let (mut world, _) = script_world(&reg, 2, 2);
    let mut schedule = Schedule::new();
    let noop = |_: &mut View<'_>, _: &SystemCtx<'_>| {};
    schedule
        .add_processor(
            processor("write_brain", Phase::PostPhysics)
                .writes_named("Brain")
                .run(noop),
        )
        .add_processor(
            processor("read_pos_by_name", Phase::PostPhysics)
                .reads_named("Pos")
                .run(noop),
        )
        .add_processor(
            processor("read_brain", Phase::PostPhysics)
                .reads_named("Brain")
                .run(noop),
        )
        .add_processor(
            processor("write_pos_typed", Phase::PostPhysics)
                .writes::<Pos>()
                .run(noop),
        )
        .add_processor(
            processor("get_ping", Phase::PostPhysics)
                .receives_named("Ping")
                .reads::<Vel>()
                .run(noop),
        );
    let sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    let post: Vec<Vec<&str>> = sim
        .schedule()
        .waves()
        .into_iter()
        .find(|(phase, _)| *phase == Phase::PostPhysics)
        .map(|(_, waves)| waves)
        .unwrap();
    assert_eq!(
        post,
        vec![
            vec!["write_brain", "read_pos_by_name", "get_ping"],
            vec!["read_brain", "write_pos_typed"],
        ]
    );
}

type Errors = Arc<Mutex<Vec<String>>>;

#[test]
fn named_view_encodes_columns_and_writes_rows_back() {
    let (reg, brain_info, _) = script_registry();
    let (mut world, ids) = script_world(&reg, 5, 3);
    world.set_json(ids[1], "Calm", Value::Null, &reg).unwrap();
    let seen: Arc<Mutex<Vec<EntityId>>> = Arc::default();
    let seen_in = seen.clone();
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("think", Phase::PostPhysics)
            .reads_named("Pos")
            .writes_named("Brain")
            .reads_named("Calm")
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                let query = NamedQuery::fetch(["Pos", "Brain"]).without(["Calm"]);
                let batch = view.batch(&query).unwrap();
                seen_in.lock().unwrap().extend(&batch.ids);
                let pos = batch.column("Pos").unwrap();
                let mut out = ColumnWriter::new(brain_info, batch.ids.len()).unwrap();
                for row in 0..batch.ids.len() {
                    let p: Pos = pos.read(row).unwrap();
                    out.write(row, &json!({"mood": p.x * 2.0, "label": "seen"}))
                        .unwrap();
                }
                view.write_batch(&batch.ids, &out.finish().unwrap())
                    .unwrap();
            }),
    );
    schedule.add_processor(processor("push", Phase::FrameEnd).writes_named("Pos").run(
        |view: &mut View<'_>, _: &SystemCtx<'_>| {
            let batch = view.batch(&NamedQuery::fetch(["Pos"])).unwrap();
            let pos = batch.column("Pos").unwrap();
            let mut out = ColumnWriter::new(Pos::type_info(), batch.ids.len()).unwrap();
            for row in 0..batch.ids.len() {
                let mut p: Pos = pos.read(row).unwrap();
                p.y += 1.0;
                out.write(row, &p).unwrap();
            }
            view.write_batch(&batch.ids, &out.finish().unwrap())
                .unwrap();
        },
    ));
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(2), 1);

    let mut expected = vec![ids[0], ids[2]];
    expected.sort();
    let mut got = seen.lock().unwrap().clone();
    got.sort();
    assert_eq!(
        got, expected,
        "Calm excluded, entities without Brain excluded"
    );
    assert_eq!(
        world.get_json(ids[2], "Brain").unwrap(),
        json!({"mood": 4.0, "label": "seen"})
    );
    assert_eq!(
        world.get_json(ids[1], "Brain").unwrap()["label"],
        json!("idle")
    );
    for (i, &id) in ids.iter().enumerate() {
        assert_eq!(world.get::<Pos>(id).unwrap().y, 1.0, "entity {i}");
    }
    world.validate_storage().unwrap();
}

#[test]
fn named_view_rejects_undeclared_names() {
    let (reg, brain_info, _) = script_registry();
    let (mut world, ids) = script_world(&reg, 2, 2);
    let errors: Errors = Arc::default();
    let log = errors.clone();
    let stranger = common::ids(3)[2];
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("misuse", Phase::PostPhysics)
            .reads_named("Pos")
            .writes_named("Brain")
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                let push = |e: QueryError| log.lock().unwrap().push(format!("{e:?}"));
                if let Err(e) = view.batch(&NamedQuery::fetch(["Vel"])) {
                    push(e);
                }
                let batch = view.batch(&NamedQuery::fetch(["Pos", "Brain"])).unwrap();
                let pos = batch.column("Pos").unwrap().clone();
                if let Err(e) = view.write_batch(&batch.ids, &pos) {
                    push(e);
                }
                let mut one = ColumnWriter::new(brain_info, 1).unwrap();
                one.write(0, &json!({"mood": 1.0, "label": "x"})).unwrap();
                let one = one.finish().unwrap();
                if let Err(e) = view.write_batch(&batch.ids, &one) {
                    push(e);
                }
                if let Err(e) = view.write_batch(&[stranger], &one) {
                    push(e);
                }
            }),
    );
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    let errors = errors.lock().unwrap().clone();
    assert_eq!(errors.len(), 4, "{errors:#?}");
    assert!(errors[0].starts_with("Undeclared"), "{}", errors[0]);
    assert!(errors[1].starts_with("ReadOnly"), "{}", errors[1]);
    assert!(errors[2].starts_with("BatchMismatch"), "{}", errors[2]);
    assert!(errors[3].starts_with("BatchMismatch"), "{}", errors[3]);
    assert_eq!(
        world.get_json(ids[0], "Brain").unwrap()["label"],
        json!("idle")
    );
}

#[test]
fn named_commands_insert_remove_and_signal() {
    let (reg, brain_info, ping_info) = script_registry();
    let (mut world, ids) = script_world(&reg, 3, 0);
    let brain_rc = component(&reg, "Brain");
    let ping_rc = component(&reg, "Ping");
    let mut schedule = Schedule::new();
    schedule.add_processor(processor("cmd", Phase::PostPhysics).reads_named("Pos").run(
        move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
            if ctx.tick.0 != 0 {
                return;
            }
            let mut targets = view.batch(&NamedQuery::fetch(["Pos"])).unwrap().ids;
            targets.sort();
            let commands = view.commands().unwrap();
            commands.insert_reflect(targets[0], brain_rc, brain(brain_info, 1.0, "new"));
            commands.remove_named(targets[1], "Pos");
            let spawned = commands.spawn();
            commands.insert_reflect(spawned, brain_rc, brain(brain_info, 9.0, "spawned"));
            let ping = DynamicValue::from_json(ping_info, json!({"n": 7})).unwrap();
            commands.signal_reflect(targets[2], ping_rc, Box::new(ping));
        },
    ));
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);

    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(
        world.get_json(sorted[0], "Brain").unwrap()["label"],
        json!("new")
    );
    assert!(world.get::<Pos>(sorted[1]).is_none());
    assert_eq!(world.len(), 4);
    let spawned: Vec<_> = world
        .entity_ids_sorted()
        .into_iter()
        .filter(|id| !ids.contains(id))
        .collect();
    assert_eq!(
        world.get_json(spawned[0], "Brain").unwrap()["label"],
        json!("spawned")
    );
    assert_eq!(
        world.entity_storage(sorted[2]).unwrap().pending_signals,
        vec![("Ping", 1)]
    );
}

#[test]
fn dynamic_signals_deliver_next_tick_in_entity_order() {
    let (reg, _, ping_info) = script_registry();
    let (mut world, ids) = script_world(&reg, 4, 0);
    let ping_rc = component(&reg, "Ping");
    let log: Arc<Mutex<Vec<(u64, EntityId, Value)>>> = Arc::default();
    let log_in = log.clone();
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("raise", Phase::PostPhysics)
            .reads_named("Pos")
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                if ctx.tick.0 != 0 {
                    return;
                }
                let mut targets = view.batch(&NamedQuery::fetch(["Pos"])).unwrap().ids;
                targets.sort();
                let commands = view.commands().unwrap();
                for (k, &id) in targets.iter().rev().enumerate() {
                    for n in [k as u32, 100 + k as u32] {
                        let ping = DynamicValue::from_json(ping_info, json!({"n": n})).unwrap();
                        commands.signal_reflect(id, ping_rc, Box::new(ping));
                    }
                    commands.signal(id, Hit(k as u32));
                }
            }),
    );
    schedule.add_processor(
        processor("receive", Phase::PrePhysics)
            .receives_named("Ping")
            .receives_named("Hit")
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                let (targets, column) = view.signal_batch(ping_info).unwrap();
                for (row, &id) in targets.iter().enumerate() {
                    let ping = column.value(row).unwrap();
                    let as_dyn: &dyn DynReflect = &ping;
                    log_in
                        .lock()
                        .unwrap()
                        .push((ctx.tick.0, id, as_dyn.to_json().unwrap()));
                }
                for (id, hit) in view.signals_named("Hit").unwrap() {
                    log_in
                        .lock()
                        .unwrap()
                        .push((ctx.tick.0, id, hit.to_json().unwrap()));
                }
                assert!(view.signals_named("Nope").is_err());
            }),
    );
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 3);

    let mut sorted = ids.clone();
    sorted.sort();
    let log = log.lock().unwrap().clone();
    assert_eq!(log.len(), 12, "two pings and one hit per entity, once");
    assert!(log.iter().all(|(tick, _, _)| *tick == 1));
    let pings: Vec<(EntityId, Value)> =
        log[..8].iter().map(|(_, id, v)| (*id, v.clone())).collect();
    let expected: Vec<(EntityId, Value)> = sorted
        .iter()
        .enumerate()
        .flat_map(|(i, &id)| {
            let k = (sorted.len() - 1 - i) as u32;
            [(id, json!({"n": k})), (id, json!({"n": 100 + k}))]
        })
        .collect();
    assert_eq!(pings, expected);
    let hits: Vec<EntityId> = log[8..].iter().map(|(_, id, _)| *id).collect();
    assert_eq!(hits, sorted);
}

fn named_hashes(threads: usize) -> Vec<WorldHash> {
    let (reg, brain_info, ping_info) = script_registry();
    let (mut world, _) = script_world(&reg, 1000, 600);
    let ping_rc = component(&reg, "Ping");
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("think", Phase::PostPhysics)
            .reads_named("Pos")
            .writes_named("Brain")
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                let batch = view.batch(&NamedQuery::fetch(["Pos", "Brain"])).unwrap();
                let (pos, old) = (batch.column("Pos").unwrap(), batch.column("Brain").unwrap());
                let mut out = ColumnWriter::new(brain_info, batch.ids.len()).unwrap();
                for row in 0..batch.ids.len() {
                    let p: Pos = pos.read(row).unwrap();
                    let before = old.value(row).unwrap();
                    let before: &dyn DynReflect = &before;
                    let mood = before.to_json().unwrap()["mood"].as_f64().unwrap() as f32;
                    let mood = mood * 0.5 + p.x * 0.001 + ctx.tick.0 as f32;
                    out.write(row, &json!({"mood": mood, "label": "t"}))
                        .unwrap();
                }
                view.write_batch(&batch.ids, &out.finish().unwrap())
                    .unwrap();
                let commands = view.commands().unwrap();
                for &id in batch.ids.iter().step_by(97) {
                    let ping =
                        DynamicValue::from_json(ping_info, json!({"n": ctx.tick.0})).unwrap();
                    commands.signal_reflect(id, ping_rc, Box::new(ping));
                }
            }),
    );
    schedule.add_processor(
        processor("drift", Phase::PostPhysics)
            .writes::<Pos>()
            .receives_named("Ping")
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                let pinged = view.signal_batch(ping_info).unwrap().0;
                view.query::<&mut Pos>()
                    .unwrap()
                    .for_each_of(&pinged, |_, p| p.y += 1.0);
            }),
    );
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    let jobs = jobs(threads);
    let mut hashes = vec![world.state_hash().unwrap()];
    for _ in 0..4 {
        sim.step(&mut world, &jobs, 1);
        hashes.push(world.state_hash().unwrap());
    }
    hashes
}

#[test]
fn named_processors_hash_identically_at_1_and_8_threads() {
    let one = named_hashes(1);
    assert_eq!(one, named_hashes(8));
    assert!(
        one.windows(2).all(|w| w[0] != w[1]),
        "every tick changes the world"
    );
}

#[test]
fn reflect_component_decodes_wire_values_for_static_and_dynamic_types() {
    let (reg, brain, _) = script_registry();
    let ops = |name: &str| {
        *reg.get_by_name(name)
            .unwrap()
            .data::<ReflectComponent>()
            .unwrap()
    };
    let pos = Pos {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let mut w = ColumnWriter::new(Pos::type_info(), 1).unwrap();
    w.write(0, &pos).unwrap();
    let column = w.finish().unwrap();
    let value = ops(Pos::type_info().name).from_wire(&column, 0).unwrap();
    assert_eq!(
        value.as_any().downcast_ref::<Pos>(),
        Some(&pos),
        "static types decode to T"
    );
    assert!(
        ops(Pos::type_info().name).from_wire(&column, 1).is_err(),
        "rows are checked"
    );

    let rows = [
        json!({"mood": 0.5, "label": "a"}),
        json!({"mood": 1.5, "label": "bb"}),
    ];
    let mut w = ColumnWriter::new(brain, 2).unwrap();
    for (row, value) in rows.iter().enumerate() {
        w.write(row, value).unwrap();
    }
    let column = w.finish().unwrap();
    let value = ops("Brain").from_wire(&column, 1).unwrap();
    assert!(
        value.as_any().is::<DynamicValue>(),
        "script types decode to dynamic values"
    );
    assert_eq!(value.to_json().unwrap(), rows[1]);
    assert!(
        ops(Pos::type_info().name).from_wire(&column, 0).is_err(),
        "a column of another type is refused"
    );
}

/// `Brain` version 2: adds `energy` (default 1.0).
fn brain_v2(reg: &mut TypeRegistry) -> &'static TypeInfo {
    register_dynamic_component(
        reg,
        &desc(
            "Brain",
            json!({"Struct": {"fields": [
                field("mood", "f32"),
                field("label", "String"),
                {"name": "energy", "ty": {"Named": "f32"}, "docs": "energy", "default": 1.0},
            ], "deny_unknown": true}}),
        ),
    )
    .unwrap()
}

fn to_version(to: &'static TypeInfo) -> impl FnMut(&DynamicValue) -> Result<DynamicValue, String> {
    move |v| {
        concordia_reflect::migrate(v, to)
            .map(|(new, _)| new)
            .map_err(|e| e.to_string())
    }
}

#[test]
fn migrate_dynamic_replaces_a_stored_type_version() {
    let (mut reg, _, ping) = script_registry();
    let (mut world, ids) = script_world(&reg, 4, 3);
    world
        .insert_dyn(
            ids[1],
            Box::new(
                DynamicValue::from_json(reg.get_by_name("Calm").unwrap().info(), json!(null))
                    .unwrap(),
            ),
            &reg,
        )
        .unwrap();
    world.signal_dynamic(
        ids[2],
        DynamicValue::from_json(ping, json!({"n": 7})).unwrap(),
    );
    let v2 = brain_v2(&mut reg);
    let value = |e: f32| json!({"mood": 0.0, "label": "idle", "energy": e});
    let stale = DynamicValue::from_json(v2, value(2.0)).unwrap();
    assert!(
        world.insert_dynamic(ids[0], stale.clone()).is_err(),
        "migrate first"
    );

    let migrated = world.migrate_dynamic(v2, to_version(v2)).unwrap();
    assert_eq!(migrated, 3, "every stored value, across archetypes");
    for &id in &ids[..3] {
        assert_eq!(
            world.get_json(id, "Brain").unwrap(),
            value(1.0),
            "added fields take defaults"
        );
    }
    world.insert_dynamic(ids[0], stale).unwrap();
    assert_eq!(
        world.get_json(ids[0], "Brain").unwrap(),
        value(2.0),
        "now the stored version"
    );
    assert_eq!(
        world.migrate_dynamic(v2, to_version(v2)).unwrap(),
        0,
        "already current"
    );

    let mut twin = World::new();
    for (i, &id) in ids.iter().enumerate() {
        twin.spawn_with_id(id).unwrap();
        twin.insert(
            id,
            Pos {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        )
        .unwrap();
        if i < 3 {
            let e = if i == 0 { 2.0 } else { 1.0 };
            twin.insert_dynamic(id, DynamicValue::from_json(v2, value(e)).unwrap())
                .unwrap();
        }
    }
    twin.insert_dyn(
        ids[1],
        Box::new(
            DynamicValue::from_json(reg.get_by_name("Calm").unwrap().info(), json!(null)).unwrap(),
        ),
        &reg,
    )
    .unwrap();
    twin.signal_dynamic(
        ids[2],
        DynamicValue::from_json(ping, json!({"n": 7})).unwrap(),
    );
    assert_eq!(
        world.state_hash().unwrap(),
        twin.state_hash().unwrap(),
        "as if stored as v2"
    );
}

#[test]
fn migrate_dynamic_is_atomic_and_refuses_storage_kind_changes() {
    let (mut reg, _, _) = script_registry();
    let (mut world, ids) = script_world(&reg, 3, 3);
    let before = world.state_hash().unwrap();
    let v2 = brain_v2(&mut reg);
    let mut calls = 0;
    let failing = world.migrate_dynamic(v2, |v| {
        calls += 1;
        match calls {
            2 => Err("the second value fails".to_owned()),
            _ => to_version(v2)(v),
        }
    });
    assert!(
        failing
            .unwrap_err()
            .to_string()
            .contains("second value fails")
    );
    assert_eq!(world.state_hash().unwrap(), before, "nothing changed");
    assert!(
        world
            .get_json(ids[0], "Brain")
            .unwrap()
            .get("energy")
            .is_none()
    );

    world
        .insert_dyn(
            ids[0],
            Box::new(
                DynamicValue::from_json(reg.get_by_name("Calm").unwrap().info(), json!(null))
                    .unwrap(),
            ),
            &reg,
        )
        .unwrap();
    let calm_v2 = register_dynamic_component(
        &mut reg,
        &desc(
            "Calm",
            json!({"Struct": {"fields": [field("level", "u8")], "deny_unknown": true}}),
        ),
    )
    .unwrap();
    let err = world
        .migrate_dynamic(calm_v2, to_version(calm_v2))
        .unwrap_err()
        .to_string();
    assert!(err.contains("tag") && err.contains("dense"), "{err}");
}

#[test]
fn migrate_dynamic_all_is_atomic_across_types() {
    let (mut reg, _, ping) = script_registry();
    let (mut world, ids) = script_world(&reg, 2, 2);
    world.signal_dynamic(
        ids[0],
        DynamicValue::from_json(ping, json!({"n": 1})).unwrap(),
    );
    let before = world.state_hash().unwrap();
    let brain = brain_v2(&mut reg);
    let ping_v2 = register_dynamic_component(
        &mut reg,
        &desc("Ping", json!({"Struct": {"fields": [field("n", "u32"), {"name": "k", "ty": {"Named": "u8"}, "docs": "k", "default": 3}], "deny_unknown": true}})),
    )
    .unwrap();
    let err = world.migrate_dynamic_all(&[brain, ping_v2], |v, to| {
        if to.name == "Ping" {
            return Err("the second type fails".to_owned());
        }
        to_version(to)(v)
    });
    assert!(err.unwrap_err().to_string().contains("second type fails"));
    assert_eq!(
        world.state_hash().unwrap(),
        before,
        "the first type was not committed alone"
    );
    let counts = world
        .migrate_dynamic_all(&[brain, ping_v2], |v, to| to_version(to)(v))
        .unwrap();
    assert_eq!(
        counts,
        [2, 0],
        "stored values per type (signals aren't counted)"
    );
    assert_eq!(
        world.get_json(ids[1], "Brain").unwrap()["energy"],
        json!(1.0)
    );
}

#[test]
fn a_world_knows_which_component_names_it_has_stored() {
    let (reg, _, _) = script_registry();
    let (mut world, ids) = script_world(&reg, 2, 1);
    assert!(world.stores("Brain") && world.stores(Pos::type_info().name));
    assert!(!world.stores("Calm"), "registered but never stored");
    world.remove_dyn(ids[0], "Brain").unwrap();
    assert!(
        world.stores("Brain"),
        "a stored type stays known (its columns keep its version)"
    );
}
