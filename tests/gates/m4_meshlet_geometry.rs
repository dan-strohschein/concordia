//! The M4 gate, meshlet geometry (M4 plan, "Milestone gate" 1; 4.13d): the canyon holds at
//! least 20 M source triangles on the meshlet path, culling draws a bounded share of its meshlets
//! and never drops visible geometry, and its frame time is reported against the 16.6 ms budget.

use concordia_render::Culling;
use concordia_tests::canyon::{self, Canyon, PATHS, SUITE, draw_paths, perf_scene, renderer};

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

/// At three frames of each path (with the history of a ride up to them), the culled image is the
/// unculled one on both draw paths; the unculled images are the goldens.
#[test]
fn canyon_no_culling_holes() {
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

/// The share of the canyon's meshlets each frame draws. The overhead (isometric) path, the game's
/// camera, draws at most 25 % on every frame (the plan's number). The ground path is reported, not
/// asserted (4.13d): its opening looks down a long open stretch of canyon, drawing 70 % on its
/// first frames and 50 % until frame 6, which only meshlet LODs (4.5, a follow-up) can bring down.
#[test]
fn canyon_culling_draws_at_most_25_percent_of_meshlets() {
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    let mut loaded = Canyon::load();
    let total = loaded.census().meshlets as f64;
    let mut over = Vec::new();
    for (scene_name, _, frames, _) in PATHS {
        let counts = loaded
            .ride(&mut r, &perf_scene(scene_name), frames, &[])
            .counts;
        let shares: Vec<f64> = counts.iter().map(|m| m.meshlets as f64 / total).collect();
        let text: Vec<String> = shares.iter().map(|s| format!("{:.0}", s * 100.0)).collect();
        println!(
            "{scene_name} meshlets drawn per frame (%): {}",
            text.join(" ")
        );
        let worst = shares.iter().copied().fold(0.0, f64::max);
        if scene_name == "m4-canyon-overhead" && worst > 0.25 {
            over.push(format!("{scene_name}: {:.1} % > 25 %", worst * 100.0));
        }
    }
    assert!(over.is_empty(), "{}", over.join("; "));
}

/// The frame budget (reported, never failing: the author deferred perf work, and GI's own
/// 2.0 ms budget is known to be missed, 4.8c): both paths' middle poses at 2560 × 1440 from
/// 1706 × 960 with VSM, probe GI and the own TAAU, each pass's median and the frame's, against
/// the 16.6 ms frame and `tests/perf/budgets/m4-canyon.budget.ron`. Says when this isn't the
/// reference machine or a release build.
#[test]
fn canyon_within_frame_budget_on_reference() {
    use concordia_perf::{HostProbe, ReferenceMachine, machine::SystemProbe};
    use concordia_render::gi::{Gi, GiTechnique};
    use concordia_render::temporal::{Temporal, TemporalBackend, Upscale};
    use concordia_render::vsm::ShadowTechnique;
    let Some(mut r) = renderer(&Default::default(), Culling::default()) else {
        return;
    };
    r.set_perf_mode(true);
    r.set_shadows(ShadowTechnique::vsm);
    r.set_gi(Gi {
        technique: GiTechnique::probes,
        ..Gi::default()
    })
    .unwrap();
    r.set_temporal(Temporal {
        backend: TemporalBackend::own,
        upscale: Upscale::X1_5,
    });
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("perf/reference.ron");
    let reference = ReferenceMachine::read(&std::fs::read_to_string(file).unwrap()).unwrap();
    let mut place = match reference.mismatch(&SystemProbe.host(r.device().caps())) {
        None => "the reference machine".to_owned(),
        Some(why) => format!("NotReference: {why}"),
    };
    if cfg!(debug_assertions) {
        place.push_str("; a debug build");
    }
    let mut loaded = Canyon::load();
    let world = concordia_render::RenderWorld::extract(&loaded.world);
    for name in ["m4-canyon-ground-taau", "m4-canyon-overhead-taau"] {
        let view = canyon::view(&perf_scene(name), 150, 300);
        let mut over = Vec::new();
        for (pass, mut ms) in loaded.pass_samples(&mut r, &world, view, (2560, 1440), 60) {
            ms.sort_by(f64::total_cmp);
            let median = ms[ms.len() / 2];
            println!("m4 gate budget ({place}) {name} {pass}: median {median:.3} ms");
            if pass == "frame" && median > 16.6 {
                over.push(format!("frame {median:.2} ms > 16.6 ms"));
            }
        }
        println!(
            "m4 gate budget ({place}) {name}: {}",
            if over.is_empty() {
                "within".to_owned()
            } else {
                over.join("; ")
            }
        );
    }
}
