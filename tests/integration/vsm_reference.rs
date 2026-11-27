//! Virtual shadow maps against the path tracer (M4 4.7a, ADR-0090, ADR-0091): `sun_courtyard`'s
//! direct-only references (one sun, no sky, `max_bounces: 1`), so the difference is shadow error.
//! Needs a GPU (required with CONCORDIA_REQUIRE_GPU=1).

use concordia_testkit::golden::{RgbaImage, flip_score};
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
    keep(name, technique, (&reference, &image));
    println!(
        "{name} {technique}: mean {:.4} p99 {:.4}",
        score.mean, score.p99
    );
    score.mean
}

/// With CONCORDIA_GOLDEN_KEEP=1, keeps the reference, `technique`'s render of `name` and their FLIP
/// map for review (`target/golden-review/m4_vsm_reference/`).
fn keep(name: &str, technique: &str, (reference, image): (&RgbaImage, &RgbaImage)) {
    if std::env::var("CONCORDIA_GOLDEN_KEEP").as_deref() != Ok("1") {
        return;
    }
    let root = concordia_tests::repo_root();
    let store = concordia_testkit::golden::Golden::new(
        root.join("tests/golden"),
        root.join("target/golden-review"),
        false,
        false,
    );
    let name = format!("{name}_{technique}");
    store
        .write_comparison("m4_vsm_reference", &name, reference, image)
        .unwrap();
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
        keep("point_seams", technique, (&reference, &image));
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

/// The soft references (M4 4.7d): the courtyard under a sun of angular radius 0.02 and the arena
/// with lights of radius 0.15 m, both cameras, with their hard twins.
const SOFT: [(&str, &str); 4] = [
    ("sun_courtyard_soft_iso", "scenes/sun_courtyard.scene.ron"),
    (
        "sun_courtyard_soft_ground",
        "scenes/sun_courtyard.scene.ron",
    ),
    (
        "spell_arena_soft_iso",
        "scenes/spell_arena_direct.scene.ron",
    ),
    (
        "spell_arena_soft_ground",
        "scenes/spell_arena_direct.scene.ron",
    ),
];

/// Soft shadows from sized lights are within mean FLIP 0.05 of the path tracer's area lights.
#[test]
fn soft_shadow_matches_path_traced_area_light() {
    if !has_gpu() {
        return;
    }
    for (name, _) in SOFT {
        let soft = flip(name, "vsm");
        assert!(soft <= 0.05, "{name}: FLIP {soft}");
    }
}

/// Soft shadows beat hard ones against the soft reference: for the courtyard's soft sun by a
/// margin, FLIP(soft) ≤ 0.8 × FLIP(hard) (a guard against a tolerance a hard shadow would pass);
/// for the arena's 0.4 m lights, no worse than hard (both sit near the tracer's noise there, and
/// PCSS under-softens large local lights against the tracer: `later-backlog.md` §12).
#[test]
fn soft_shadows_beat_hard_ones_on_soft_references() {
    if !has_gpu() {
        return;
    }
    let root = concordia_tests::repo_root();
    std::env::set_current_dir(&root).expect("the repository root");
    let golden = root.join("tests/golden");
    for (name, hard_scene) in SOFT {
        let soft = flip(name, "vsm");
        let (image, reference) = concordia_tests::reference::render_scene_against(
            name,
            Some(hard_scene),
            &golden,
            "vsm",
        )
        .unwrap();
        keep(name, "vsm_hard", (&reference, &image));
        let hard = flip_score(&reference, &image).unwrap().mean;
        println!("{name}: soft {soft:.4}, hard {hard:.4}");
        let margin = if name.starts_with("sun_") { 0.8 } else { 1.0 };
        assert!(
            soft <= margin * hard,
            "{name}: soft {soft} against hard {hard}"
        );
    }
}
