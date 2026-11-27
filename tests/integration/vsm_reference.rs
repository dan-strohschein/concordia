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
