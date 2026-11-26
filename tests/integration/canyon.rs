//! The M4 canyon (phase 4.4c; Phase 4.4's done criterion): `examples/m4-canyon` is made from
//! fixed seeds and remade byte for byte, holds at least 20 M source triangles all on the meshlet
//! path, and along its two scripted camera paths the meshlet counts are the same between runs and
//! between the two draw paths, frame by frame, while culling never changes the image. With the own
//! TAAU (phase 4.6's done criterion) it renders 2560 × 1440 from 1706 × 960.

use concordia_gen::canyon::{CameraKey, Canyon as Spec, Family};
use concordia_gen::{GenContext, ProviderRegistry};
use concordia_render::Culling;
use concordia_tests::canyon::{
    self, Canyon, SUITE, draw_paths, first_difference, perf_scene, renderer, without_draws,
};

/// The two paths with the frames each test draws and the frames kept as images.
const PATHS: [(&str, &str, u32, [u32; 3]); 2] = [
    ("m4-canyon-ground", "canyon_ground", 32, [0, 16, 31]),
    ("m4-canyon-overhead", "canyon_overhead", 16, [0, 8, 15]),
];

/// Every generated mesh (and the sky map) is remade from its committed recipe with exactly the recorded bytes (on
/// any OS: the generators use no platform maths); another seed makes another rock; and the
/// committed scene and perf scenes are the canyon spec's layout and camera paths.
#[test]
fn canyon_is_deterministic_for_its_seeds() {
    let db = canyon::asset_db();
    let recipes = canyon::recipes(&db);
    let spec = Spec::M4;
    let paths: Vec<String> = spec.assets().into_iter().map(|a| a.path).collect();
    let mut recorded: Vec<String> = recipes.iter().map(|(p, _)| p.clone()).collect();
    recorded.sort();
    let mut want = paths.clone();
    want.push("assets/sky/sky.png".into());
    want.sort();
    assert_eq!(
        recorded, want,
        "one recipe per canyon mesh, and the sky map"
    );
    let registry = ProviderRegistry::procedural();
    let project = canyon::project();
    let ctx = GenContext { project: &project };
    std::thread::scope(|s| {
        for chunk in recipes.chunks(5) {
            let (registry, ctx) = (&registry, &ctx);
            s.spawn(move || {
                for (path, p) in chunk {
                    concordia_gen::restore::regenerate(registry, p, ctx)
                        .unwrap_or_else(|e| panic!("{path}: {e}"));
                }
            });
        }
    });
    let stone = spec
        .assets()
        .into_iter()
        .find(|a| a.family == Some(Family::Stone))
        .unwrap();
    let mesh = |seed| concordia_gen::mesh::build(&stone.request, seed).unwrap();
    assert_ne!(mesh(stone.seed).positions, mesh(stone.seed + 1).positions);

    let loaded = Canyon::load();
    let mut placed = loaded.instances();
    placed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut layout = spec.layout();
    layout.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(placed.len(), layout.len(), "one entity per placement");
    for ((name, renderer), p) in placed.iter().zip(&layout) {
        assert_eq!(name, &p.name);
        let id = match &renderer.mesh {
            concordia_assets::AssetRef::Asset { id, .. } => *id,
            other => panic!("{name}: {other}"),
        };
        let path = loaded
            .assets
            .path_of(id)
            .map(|p| p.to_string_lossy().replace('\\', "/"));
        assert_eq!(path.as_deref(), Some(p.asset.as_str()), "{name}'s mesh");
        let id = loaded.world.entity_ids_sorted().into_iter().find(|&e| {
            loaded
                .world
                .get::<concordia_scene::builtin::Name>(e)
                .map(|n| &n.0)
                == Some(name)
        });
        let t = loaded
            .world
            .get::<concordia_scene::builtin::Transform>(id.unwrap())
            .unwrap();
        assert_eq!(t.translation.to_array(), p.translation, "{name}");
        assert_eq!(t.rotation.to_array(), p.rotation, "{name}");
        assert_eq!(t.scale.to_array(), [p.scale; 3], "{name}");
    }
    let keys = |name| -> Vec<CameraKey> {
        perf_scene(name)
            .path
            .iter()
            .map(|k| CameraKey {
                eye: k.eye,
                target: k.target,
            })
            .collect()
    };
    assert_eq!(keys("m4-canyon-ground"), spec.ground_path());
    assert_eq!(keys("m4-canyon-overhead"), spec.overhead_path());
}

/// The sum over instances of each mesh's triangles is at least 20,000,000, and every instance's
/// mesh has meshlets (so all of it takes the meshlet path).
#[test]
fn canyon_has_at_least_20m_source_triangles() {
    let mut loaded = Canyon::load();
    let census = loaded.census();
    println!("{census:?}");
    assert!(census.triangles >= 20_000_000, "{census:?}");
    assert!(census.without_meshlets.is_empty(), "{census:?}");
    assert_eq!(census.instances, 320);
}

/// Along both paths, the meshlet counts of every frame are equal between two runs (fresh
/// renderers) and between the mesh-shader and compute paths (but for the draw commands); every
/// frame draws something, sees the whole scene as candidates, and only ever narrows it.
#[test]
fn canyon_meshlet_counts_are_stable_on_both_paths() {
    let paths = draw_paths();
    if paths.is_empty() {
        return;
    }
    let mut loaded = Canyon::load();
    let total = loaded.census().meshlets;
    for (scene_name, _, frames, _) in PATHS {
        let scene = perf_scene(scene_name);
        let mut per_path = Vec::new();
        for mask in &paths {
            let mut runs = Vec::new();
            for _ in 0..2 {
                let mut r = renderer(mask, Culling::default()).unwrap();
                runs.push(loaded.ride(&mut r, &scene, frames, &[]).counts);
            }
            let path = canyon::raster_path(&renderer(mask, Culling::default()).unwrap());
            if let Some(d) = first_difference(&runs[0], &runs[1]) {
                panic!("{scene_name} on {path:?}: the runs differ at {d}");
            }
            for (i, m) in runs[0].iter().enumerate() {
                assert!(m.meshlets > 0, "{scene_name} frame {i}: {m:?}");
                assert_eq!(m.candidates, total, "{scene_name} frame {i}: {m:?}");
                let chain = [
                    m.candidates,
                    m.after_instance_frustum,
                    m.after_frustum,
                    m.after_cone,
                    m.after_occlusion,
                ];
                assert!(
                    chain.windows(2).all(|w| w[0] >= w[1]),
                    "{scene_name} frame {i}: {m:?}"
                );
                assert_eq!(m.meshlets, m.after_occlusion, "{m:?}");
                assert_eq!(m.meshlets, m.drawn_early + m.drawn_late, "{m:?}");
            }
            let first = runs[0][frames as usize / 2];
            println!("{scene_name} on {path:?}, frame {}: {first:?}", frames / 2);
            per_path.push(runs.swap_remove(0));
        }
        if let [a, b] = &per_path[..] {
            let strip = |v: &[concordia_render::MeshletCounts]| -> Vec<_> {
                v.iter().copied().map(without_draws).collect()
            };
            if let Some(d) = first_difference(&strip(a), &strip(b)) {
                panic!("{scene_name}: the mesh-shader and compute paths differ at {d}");
            }
        }
    }
}

/// At three frames of each path (with the history of a ride up to them), the culled image is the
/// unculled one on both draw paths; the unculled images are the goldens.
#[test]
fn canyon_culling_matches_culling_off() {
    use concordia_testkit::golden::{Comparison, Golden, RgbaImage, flip_score};
    let paths = draw_paths();
    if paths.is_empty() {
        return;
    }
    let mut loaded = Canyon::load();
    let mut missed = Vec::new();
    let backend = concordia_rhi_select::choose_from_env(Default::default()).unwrap();
    let rgba =
        |i: &concordia_render::Image| RgbaImage::new(i.width, i.height, i.rgba.clone()).unwrap();
    for (scene_name, golden, frames, keep) in PATHS {
        let scene = perf_scene(scene_name);
        for mask in &paths {
            let mut on = renderer(mask, Culling::default()).unwrap();
            let culled = loaded.ride(&mut on, &scene, frames, &keep).images;
            let mut off = renderer(mask, Culling::OFF).unwrap();
            let unculled = loaded.stills(&mut off, &scene, frames, &keep);
            let path = canyon::raster_path(&on);
            for i in keep {
                let name = format!("{golden}_{i:03}");
                let tolerance = Golden::from_env()
                    .comparison_tolerance_on(Some(backend), SUITE, &name, Comparison::Culling)
                    .unwrap()
                    .unwrap_or_else(|| panic!("a culling entry for {name}"));
                let score = flip_score(&rgba(&unculled[&i]), &rgba(&culled[&i])).unwrap();
                println!("{name} on {path:?}: culling on vs off {score:?}");
                assert!(
                    tolerance.accepts(&score),
                    "{name} on {path:?}: {score:?} > {tolerance:?}"
                );
                let checked =
                    Golden::from_env().check_on(Some(backend), SUITE, &name, &rgba(&unculled[&i]));
                if let Err(e) = checked {
                    missed.push(format!("{name} on {path:?}: {e}"));
                }
            }
        }
    }
    assert!(missed.is_empty(), "{}", missed.join("\n"));
}

/// Phase 4.6's done criterion (4.6 plan Q11): the ground path's 32 frames at 2560 × 1440 from
/// 1706 × 960 with the own TAAU (1.5×). Every frame is drawn by the own resolve at those sizes and
/// shows the canyon (not black, not flat).
#[test]
fn canyon_taau_runs_at_1440p() {
    use concordia_render::temporal::{Temporal, TemporalBackend, Upscale};
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    r.set_temporal(Temporal {
        backend: TemporalBackend::own,
        upscale: Upscale::X1_5,
    });
    let mut loaded = Canyon::load();
    let world = concordia_render::RenderWorld::extract(&loaded.world);
    let scene = perf_scene("m4-canyon-ground");
    for i in 0..32 {
        let view = canyon::view(&scene, i, 32);
        let image = loaded.render_sized(&mut r, &world, view, (2560, 1440));
        canyon::keep_for_review("canyon_ground_1440p", i, &image);
        let t = r.stats().temporal;
        assert_eq!(t.backend, TemporalBackend::own, "frame {i}");
        assert_eq!(
            (t.internal, t.output),
            ([1706, 960], [2560, 1440]),
            "frame {i}"
        );
        let luma: Vec<f32> = image.rgba.chunks(4).map(|p| p[1] as f32).collect();
        let mean = luma.iter().sum::<f32>() / luma.len() as f32;
        let spread = luma.iter().map(|l| (l - mean).abs()).sum::<f32>() / luma.len() as f32;
        assert!(
            mean > 20.0 && spread > 5.0,
            "frame {i}: mean {mean}, spread {spread}"
        );
    }
}

/// The own TAAU on both paths: a still middle pose after 32 frames at 1280 × 720 from 853 × 480,
/// against the `m4_temporal` goldens.
#[test]
fn canyon_taau_goldens() {
    use concordia_render::temporal::{Temporal, TemporalBackend, Upscale};
    use concordia_testkit::golden::{Golden, RgbaImage};
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    r.set_temporal(Temporal {
        backend: TemporalBackend::own,
        upscale: Upscale::X1_5,
    });
    let backend = concordia_rhi_select::choose_from_env(Default::default()).unwrap();
    let mut loaded = Canyon::load();
    let world = concordia_render::RenderWorld::extract(&loaded.world);
    let mut missed = Vec::new();
    for (scene_name, golden, frames, keep) in PATHS {
        let scene = perf_scene(scene_name);
        let view = canyon::view(&scene, keep[1], frames);
        r.reset_motion_history();
        let mut image = None;
        for _ in 0..32 {
            image = Some(loaded.render_sized(&mut r, &world, view.clone(), (1280, 720)));
        }
        let image = image.unwrap();
        assert_eq!(r.stats().temporal.internal, [853, 480]);
        let name = format!("{golden}_taau");
        let rgba = RgbaImage::new(image.width, image.height, image.rgba.clone()).unwrap();
        if let Err(e) = Golden::from_env().check_on(Some(backend), "m4_temporal", &name, &rgba) {
            missed.push(format!("{name}: {e}"));
        }
    }
    assert!(missed.is_empty(), "{}", missed.join("\n"));
}
