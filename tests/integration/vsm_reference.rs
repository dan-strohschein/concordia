//! Virtual shadow maps against the path tracer (M4 4.7a, ADR-0090, ADR-0091): `sun_courtyard`'s
//! direct-only references (one sun, no sky, `max_bounces: 1`), so the difference is shadow error.
//! Needs a GPU (required with CONCORDIA_REQUIRE_GPU=1).

use concordia_testkit::golden::flip_score;
use concordia_tests::reference::render_against;

const VIEWS: [&str; 2] = ["sun_courtyard_iso", "sun_courtyard_ground"];

fn has_gpu() -> bool {
    let ok = concordia_rhi_metal::probe().is_ok();
    assert!(
        ok || !concordia_testkit::gpu_required(),
        "CONCORDIA_REQUIRE_GPU=1 but no GPU"
    );
    ok
}

/// The FLIP mean of `technique`'s render of `name` against its reference.
fn flip(name: &str, technique: &str) -> f32 {
    let root = concordia_tests::repo_root();
    std::env::set_current_dir(&root).expect("the repository root");
    let golden = root.join("tests/golden");
    let (image, reference) = render_against(name, &golden, technique).unwrap();
    let score = flip_score(&reference, &image).unwrap();
    // CONCORDIA_GOLDEN_KEEP=1 keeps the reference, the render and their FLIP map for review.
    if std::env::var("CONCORDIA_GOLDEN_KEEP").as_deref() == Ok("1") {
        let review = root.join("target/golden-review");
        let store = concordia_testkit::golden::Golden::new(golden.clone(), review, false, false);
        let name = format!("{name}_{technique}");
        store
            .write_comparison("m4_vsm_reference", &name, &reference, &image)
            .unwrap();
    }
    println!(
        "{name} {technique}: mean {:.4} p99 {:.4}",
        score.mean, score.p99
    );
    score.mean
}

/// VSM's direct light is within mean FLIP 0.05 of the path tracer's on both views.
#[test]
fn vsm_direct_shadow_matches_path_traced_direct_lighting() {
    if !has_gpu() {
        return;
    }
    for name in VIEWS {
        let vsm = flip(name, "vsm");
        assert!(vsm <= 0.05, "{name}: FLIP {vsm}");
    }
}

/// VSM is closer to the reference than CSM on the same views (thin casters and contact shadows),
/// by a margin: FLIP(VSM) ≤ 0.8 × FLIP(CSM). A guard against a tolerance loose enough to pass a
/// CSM-quality result.
#[test]
fn vsm_beats_csm_on_contact_shadows() {
    if !has_gpu() {
        return;
    }
    for name in VIEWS {
        let (vsm, csm) = (flip(name, "vsm"), flip(name, "csm"));
        assert!(vsm <= 0.8 * csm, "{name}: vsm {vsm}, csm {csm}");
    }
}

/// `spell_arena_direct` (M4 4.7c): four shadowed spot lights and four shadowed point lights, no
/// sun, both cameras.
const ARENA: [&str; 2] = ["spell_arena_direct_iso", "spell_arena_direct_ground"];

/// Spot and point light shadows on virtual shadow maps are within mean FLIP 0.05 of the path
/// tracer's direct light (the plan's bound).
#[test]
fn spot_and_point_shadows_match_path_traced() {
    if !has_gpu() {
        return;
    }
    for name in ARENA {
        let vsm = flip(name, "vsm");
        assert!(vsm <= 0.05, "{name}: FLIP {vsm}");
    }
}

/// Local shadows beat no local shadows (under `csm` local lights are unshadowed) by a margin,
/// FLIP(VSM) ≤ 0.8 × FLIP(unshadowed): a guard against a tolerance that would pass with no
/// shadows at all.
#[test]
fn local_shadows_beat_unshadowed_lights() {
    if !has_gpu() {
        return;
    }
    for name in ARENA {
        let (vsm, unshadowed) = (flip(name, "vsm"), flip(name, "csm"));
        assert!(
            vsm <= 0.8 * unshadowed,
            "{name}: VSM {vsm} against unshadowed {unshadowed}"
        );
    }
}

/// A point light's six cube faces meet without seams (M4 4.7c): in `point_seams` (a closed room, a
/// shadowed point light in its middle, beams whose shadows run across the faces' edges) VSM is
/// within mean FLIP 0.05 of the path tracer, and its worst percentile stays far under the
/// unshadowed image's (a lit crack along a face edge would show up there).
#[test]
fn cube_faces_have_no_seams() {
    if !has_gpu() {
        return;
    }
    let root = concordia_tests::repo_root();
    std::env::set_current_dir(&root).expect("the repository root");
    let golden = root.join("tests/golden");
    let score = |technique: &str| {
        let (image, reference) = render_against("point_seams", &golden, technique).unwrap();
        flip_score(&reference, &image).unwrap()
    };
    let (vsm, unshadowed) = (score("vsm"), score("csm"));
    println!(
        "point_seams vsm: mean {:.4} p99 {:.4}; unshadowed: mean {:.4} p99 {:.4}",
        vsm.mean, vsm.p99, unshadowed.mean, unshadowed.p99
    );
    assert!(vsm.mean <= 0.05, "mean {}", vsm.mean);
    assert!(
        vsm.p99 <= 0.25 * unshadowed.p99,
        "p99 {} against {}",
        vsm.p99,
        unshadowed.p99
    );
}
