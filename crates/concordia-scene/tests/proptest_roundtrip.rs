//! Property tests: arbitrary scenes round-trip; the reader never panics (M0 phase 0.5).

mod common;

use std::collections::BTreeMap;

use common::{Gadget, Inner, Mode, registry};
use concordia_core::id::{DeterministicIdGen, IdSource};
use concordia_core::math::{Quat, Vec2, Vec3};
use concordia_scene::builtin::{Name, Transform};
use concordia_scene::{SceneMeta, read_scene, snapshot_world, write_scene};
use concordia_terra::World;
use proptest::prelude::*;

fn f32s() -> impl Strategy<Value = f32> {
    prop_oneof![
        any::<f32>().prop_filter("finite", |x| x.is_finite()),
        Just(-0.0f32),
        Just(f32::MIN_POSITIVE / 8.0),
    ]
}

fn vec3() -> impl Strategy<Value = Vec3> {
    (f32s(), f32s(), f32s()).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9 ]{0,8}",
        ".{0,8}",
        Just("q\"b\\n\n\t\u{1}é".to_owned())
    ]
}

fn mode() -> impl Strategy<Value = Mode> {
    prop_oneof![
        Just(Mode::Idle),
        f32s().prop_map(Mode::Walk),
        (f32s(), f32s()).prop_map(|(a, b)| Mode::Jump(a, b)),
        (prop::collection::vec(vec3(), 0..3), any::<bool>())
            .prop_map(|(points, looping)| Mode::Patrol { points, looping }),
    ]
}

prop_compose! {
    fn gadget()(
        mode in mode(),
        label in prop::option::of(text()),
        tags in prop::collection::vec(text(), 0..3),
        stats in prop::collection::btree_map(text(), any::<f64>().prop_filter("finite", |x| x.is_finite()), 0..3),
        offset in prop::option::of((f32s(), f32s()).prop_map(|(x, y)| Vec2::new(x, y))),
        count in any::<u32>(),
        inner in (any::<i64>(), text()).prop_map(|(a, note)| Inner { a, note }),
        pair in any::<(i32, bool)>(),
    ) -> Gadget {
        Gadget { mode, label, tags, stats: stats.into_iter().collect::<BTreeMap<_, _>>(), offset, count, inner, pair }
    }
}

#[derive(Debug, Clone)]
struct Spec {
    name: Option<String>,
    transform: Option<(Vec3, Vec3)>,
    gadget: Option<Gadget>,
}

fn spec() -> impl Strategy<Value = Spec> {
    (
        prop::option::of(text()),
        prop::option::of((vec3(), vec3())),
        prop::option::of(gadget()),
    )
        .prop_map(|(name, transform, gadget)| Spec {
            name,
            transform,
            gadget,
        })
}

fn build(specs: &[Spec], seed: u64) -> (World, SceneMeta) {
    let mut ids = DeterministicIdGen::new(seed);
    let mut world = World::new();
    for spec in specs {
        let id = world.spawn(&mut ids).unwrap();
        if let Some(name) = &spec.name {
            world.insert(id, Name(name.clone())).unwrap();
        }
        if let Some((translation, scale)) = spec.transform {
            world
                .insert(
                    id,
                    Transform {
                        translation,
                        rotation: Quat::IDENTITY,
                        scale,
                    },
                )
                .unwrap();
        }
        if let Some(gadget) = &spec.gadget {
            world.insert(id, gadget.clone()).unwrap();
        }
    }
    (
        world,
        SceneMeta {
            id: ids.next_id().unwrap(),
            name: "P".into(),
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn arbitrary_scene_text_roundtrip(specs in prop::collection::vec(spec(), 0..50), seed in any::<u64>()) {
        let reg = registry();
        let (world, meta) = build(&specs, seed);
        let doc = snapshot_world(&meta, &world).unwrap();
        let text = write_scene(&doc).unwrap();
        let read = read_scene(&text, &reg).map_err(|e| TestCaseError::fail(format!("{e}\n{text}")))?;
        prop_assert_eq!(&read, &doc);
        let mut loaded = World::new();
        concordia_scene::load_into_world(&read, &mut loaded, &reg).unwrap();
        prop_assert_eq!(loaded.state_hash().unwrap(), world.state_hash().unwrap());
    }

    #[test]
    fn canonical_write_is_fixed_point(specs in prop::collection::vec(spec(), 0..10), seed in any::<u64>()) {
        let reg = registry();
        let (world, meta) = build(&specs, seed);
        let text = write_scene(&snapshot_world(&meta, &world).unwrap()).unwrap();
        prop_assert_eq!(write_scene(&read_scene(&text, &reg).unwrap()).unwrap(), text);
    }

    #[test]
    fn reader_never_panics_on_arbitrary_text(text in "\\PC{0,200}") {
        let _ = read_scene(&text, &registry());
    }

    #[test]
    fn reader_never_panics_on_mutated_scenes(cut in 0usize..2000, insert in "[(){}\\[\\]:,\"a-z0-9.#-]{0,3}") {
        let base = common::fixture("gadget.scene.ron");
        let mut at = cut.min(base.len());
        while !base.is_char_boundary(at) {
            at -= 1;
        }
        let mutated = format!("{}{}{}", &base[..at], insert, &base[at..]);
        let _ = read_scene(&mutated, &registry());
    }
}

#[test]
fn deeply_nested_input_is_an_error_not_a_stack_overflow() {
    let text = format!(
        "Scene(format: 1, id: {}{})",
        "[".repeat(100_000),
        "]".repeat(100_000)
    );
    assert!(read_scene(&text, &registry()).is_err());
}
