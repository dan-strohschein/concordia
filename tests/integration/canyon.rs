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
    // The 1440p TAAU twins (4.6 plan Q13): the same paths, from 1706 × 960 with the own resolve.
    use concordia_render::temporal::TemporalBackend;
    for name in ["m4-canyon-ground", "m4-canyon-overhead"] {
        let (plain, twin) = (perf_scene(name), perf_scene(&format!("{name}-taau")));
        assert_eq!(twin.path, plain.path, "{name}");
        assert_eq!((twin.width, twin.height), (2560, 1440), "{name}");
        assert_eq!(twin.fov_y, plain.fov_y, "{name}");
        let t = twin.temporal.expect("a temporal setting");
        assert_eq!(
            (t.backend, t.upscale),
            (TemporalBackend::own, 1.5),
            "{name}"
        );
    }
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
/// 1706 × 960 at 1.5×, with the own TAAU and, where the device has it, MetalFX. Every frame is drawn
/// by the backend asked for at those sizes and shows the canyon (not black, not flat), and the two
/// backends' last frames are within FLIP 0.06.
#[test]
fn canyon_taau_runs_at_1440p() {
    use concordia_render::temporal::{Temporal, TemporalBackend, Upscale};
    use concordia_testkit::golden::{RgbaImage, flip_score};
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    let mut loaded = Canyon::load();
    let world = concordia_render::RenderWorld::extract(&loaded.world);
    let scene = perf_scene("m4-canyon-ground");
    let mut backends = vec![TemporalBackend::own];
    match r.device().caps().metalfx_temporal {
        true => backends.push(TemporalBackend::metalfx),
        false => eprintln!("MetalFX skipped: this device has no MetalFX temporal scaler"),
    }
    let mut last = Vec::new();
    for backend in backends {
        r.set_temporal(Temporal {
            backend,
            upscale: Upscale::X1_5,
        });
        r.reset_motion_history();
        last.push(ride_1440p(&mut r, &mut loaded, &world, &scene, backend));
    }
    if let [own, metalfx] = &last[..] {
        let rgba = |i: &concordia_render::Image| {
            RgbaImage::new(i.width, i.height, i.rgba.clone()).unwrap()
        };
        let score = flip_score(&rgba(own), &rgba(metalfx)).unwrap();
        eprintln!("canyon frame 31, MetalFX against own: {score:?}");
        assert!(
            score.mean <= 0.06,
            "MetalFX against own at 1440p: {score:?}"
        );
    }
}

/// The ground path's 32 frames at 2560 × 1440 with `backend` (set on `r`), each checked; the last.
fn ride_1440p(
    r: &mut concordia_render::Renderer,
    loaded: &mut Canyon,
    world: &concordia_render::RenderWorld,
    scene: &concordia_perf::PerfScene,
    backend: concordia_render::temporal::TemporalBackend,
) -> concordia_render::Image {
    let mut last = None;
    for i in 0..32 {
        let view = canyon::view(scene, i, 32);
        let image = loaded.render_sized(r, world, view, (2560, 1440));
        canyon::keep_for_review(&format!("canyon_ground_1440p_{backend:?}"), i, &image);
        let t = r.stats().temporal;
        assert_eq!(t.backend, backend, "frame {i}");
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
            "{backend:?} frame {i}: mean {mean}, spread {spread}"
        );
        last = Some(image);
    }
    last.expect("32 frames")
}

/// Phase 4.6's `temporal_pass_budget`, recording only (4.6 plan Q13): the own resolve's GPU time
/// on the ground path's middle pose at 2560 × 1440 from 1706 × 960, 120 frames after 30 warm-up,
/// printed as median and p95 against the provisional 0.8 ms. It passes either way (the assertion
/// lands with the canyon budget file at M4's end) and says when this isn't the reference machine.
#[test]
fn temporal_pass_budget() {
    use concordia_perf::{HostProbe, ReferenceMachine, Summary, machine::SystemProbe};
    use concordia_render::temporal::{Temporal, TemporalBackend, Upscale};
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    r.set_perf_mode(true);
    r.set_temporal(Temporal {
        backend: TemporalBackend::own,
        upscale: Upscale::X1_5,
    });
    let mut loaded = Canyon::load();
    let world = concordia_render::RenderWorld::extract(&loaded.world);
    let scene = perf_scene("m4-canyon-ground-taau");
    let view = canyon::view(&scene, 150, 300);
    let samples = loaded.pass_samples(&mut r, &world, view, (2560, 1440), 120);
    let Some(resolve) = samples.get("temporal.resolve").and_then(|s| Summary::of(s)) else {
        println!(
            "temporal_pass_budget: not measured ({:?})",
            r.stats().pass_timing
        );
        return;
    };
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("perf/reference.ron");
    let reference = ReferenceMachine::read(&std::fs::read_to_string(file).unwrap()).unwrap();
    let place = match reference.mismatch(&SystemProbe.host(r.device().caps())) {
        None => "the reference machine".to_owned(),
        Some(why) => format!("NotReference: {why}"),
    };
    let verdict = if resolve.p95_ms <= 0.8 {
        "within"
    } else {
        "over"
    };
    println!(
        "temporal_pass_budget ({place}): temporal.resolve median {:.3} ms, p95 {:.3} ms; {verdict} the provisional 0.8 ms",
        resolve.median_ms, resolve.p95_ms
    );
}

/// Timings only, for the As built (4.6 plan Q13): the ground path's middle pose at 2560 × 1440 with
/// temporal off, the own resolve and MetalFX (4.6b) at 1.0×, 1.5× and 2.0×; each pass's median and the
/// frame's (MetalFX's own work shows only in the frame).
/// Run on the reference Mac:
/// `cargo nextest run -p concordia-tests --release --test canyon --run-ignored only -E 'test(canyon_temporal_timings)' --no-capture`.
#[test]
#[ignore = "timings for the As built, run by hand in release"]
fn canyon_temporal_timings() {
    use concordia_render::temporal::{Temporal, TemporalBackend, Upscale};
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    r.set_perf_mode(true);
    let mut loaded = Canyon::load();
    let world = concordia_render::RenderWorld::extract(&loaded.world);
    let view = canyon::view(&perf_scene("m4-canyon-ground-taau"), 150, 300);
    let runs = [
        ("off", TemporalBackend::off, Upscale::X1),
        ("own 1.0×", TemporalBackend::own, Upscale::X1),
        ("own 1.5×", TemporalBackend::own, Upscale::X1_5),
        ("own 2.0×", TemporalBackend::own, Upscale::X2),
        ("metalfx 1.0×", TemporalBackend::metalfx, Upscale::X1),
        ("metalfx 1.5×", TemporalBackend::metalfx, Upscale::X1_5),
        ("metalfx 2.0×", TemporalBackend::metalfx, Upscale::X2),
    ];
    for (label, backend, upscale) in runs {
        r.set_temporal(Temporal { backend, upscale });
        for (name, mut ms) in loaded.pass_samples(&mut r, &world, view.clone(), (2560, 1440), 150) {
            ms.sort_by(f64::total_cmp);
            println!("canyon {label} {name}: median {:.3} ms", ms[ms.len() / 2]);
        }
    }
}

/// The own TAAU on both paths, and MetalFX on the ground path where the device has it: a still
/// middle pose after 32 frames at 1280 × 720 from 853 × 480, against the `m4_temporal` goldens.
#[test]
fn canyon_taau_goldens() {
    use concordia_render::temporal::{Temporal, TemporalBackend, Upscale};
    use concordia_testkit::golden::{Golden, RgbaImage};
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    let backend = concordia_rhi_select::choose_from_env(Default::default()).unwrap();
    let mut loaded = Canyon::load();
    let world = concordia_render::RenderWorld::extract(&loaded.world);
    let mut runs: Vec<_> = PATHS
        .iter()
        .map(|path| (TemporalBackend::own, path, "taau"))
        .collect();
    if r.device().caps().metalfx_temporal {
        runs.push((TemporalBackend::metalfx, &PATHS[0], "metalfx"));
    }
    let mut missed = Vec::new();
    for (temporal, (scene_name, golden, frames, keep), suffix) in runs {
        r.set_temporal(Temporal {
            backend: temporal,
            upscale: Upscale::X1_5,
        });
        let scene = perf_scene(scene_name);
        let view = canyon::view(&scene, keep[1], *frames);
        r.reset_motion_history();
        let mut image = None;
        for _ in 0..32 {
            image = Some(loaded.render_sized(&mut r, &world, view.clone(), (1280, 720)));
        }
        let image = image.unwrap();
        let t = r.stats().temporal;
        assert_eq!((t.backend, t.internal), (temporal, [853, 480]));
        let name = format!("{golden}_{suffix}");
        let rgba = RgbaImage::new(image.width, image.height, image.rgba.clone()).unwrap();
        if let Err(e) = Golden::from_env().check_on(Some(backend), "m4_temporal", &name, &rgba) {
            missed.push(format!("{name}: {e}"));
        }
    }
    assert!(missed.is_empty(), "{}", missed.join("\n"));
}

/// The canyon runs on virtual shadow maps by default (M4 4.7b, Phase 4.7's done criterion, first
/// half): its manifest asks for `vsm`, its four perf scenes say so, and a renderer set up from the
/// manifest runs VSM with no fallback.
#[test]
fn canyon_uses_vsm() {
    use concordia_render::vsm::ShadowTechnique;
    let project = concordia_app::Project::load(&canyon::project()).expect("the canyon project");
    let render = project.manifest.render.clone().unwrap_or_default();
    assert_eq!(render.shadows, ShadowTechnique::vsm);
    for name in [
        "m4-canyon-ground",
        "m4-canyon-overhead",
        "m4-canyon-ground-taau",
        "m4-canyon-overhead-taau",
    ] {
        assert_eq!(
            perf_scene(name).shadows,
            Some(ShadowTechnique::vsm),
            "{name}"
        );
    }
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    r.set_shadows(render.shadows);
    r.set_vsm_settings(render.vsm).unwrap();
    let mut loaded = Canyon::load();
    let scene = perf_scene("m4-canyon-overhead");
    loaded.stills(&mut r, &scene, 16, &[8]);
    let s = r.stats().shadows;
    assert_eq!(
        (s.technique, s.reason.as_deref()),
        (ShadowTechnique::vsm, None),
        "{s:?}"
    );
}

/// Both camera paths render on cached virtual shadow maps: no pairs dropped, no page starved, and
/// after the first frame most resident pages are kept.
#[test]
fn canyon_paths_render_with_vsm_cached() {
    use concordia_render::vsm::ShadowTechnique;
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    r.set_shadows(ShadowTechnique::vsm);
    let mut loaded = Canyon::load();
    let world = concordia_render::RenderWorld::extract(&loaded.world);
    for (scene_name, _, frames, _) in PATHS {
        let scene = perf_scene(scene_name);
        for i in 0..frames {
            loaded.render(&mut r, &world, canyon::view(&scene, i, frames));
            let s = r.stats().shadows;
            assert_eq!(s.technique, ShadowTechnique::vsm, "{scene_name} {i}: {s:?}");
            assert_eq!(
                (s.page_draws_dropped, s.pages_starved),
                (0, 0),
                "{scene_name} {i}: {s:?}"
            );
            if i > 0 {
                assert!(
                    s.pages_cached * 2 > s.pages_allocated,
                    "{scene_name} {i}: {s:?}"
                );
            }
        }
    }
}
