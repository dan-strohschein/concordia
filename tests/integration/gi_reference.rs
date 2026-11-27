//! Probe GI against the path tracer's multi-bounce references (M4 4.8b): `cornell_spell` and
//! `iso_arena` (`tests/golden/m4_reference`, `max_bounces: 8`).

use concordia_testkit::golden::{RgbaImage, flip_score};
use serde_json::{Value, json};

fn has_gpu() -> bool {
    match concordia_rhi_select::new_headless(concordia_rhi::Backend::Metal) {
        Ok(_) => true,
        Err(e) => {
            assert!(!concordia_testkit::gpu_required(), "{e}");
            false
        }
    }
}

/// How each reference runs GI: `iso_arena` (the isometric arena GI is tuned for) with the
/// defaults; `cornell_spell`, a 5 m room, with a room-sized dense volume (coarse probes can't
/// resolve a small interior; a level author places such a volume, as with DDGI volumes elsewhere).
fn gi_params(name: &str) -> Value {
    match name {
        "cornell_spell" => json!({"technique": "probes", "spacing": 0.5, "rays": 512,
            "probes": [12, 12, 12], "center": [0.0, 2.5, 0.0]}),
        _ => json!({"technique": "probes"}),
    }
}

/// Mean FLIP of reference `name` rendered with GI on or off, the images kept on request
/// (`CONCORDIA_GOLDEN_KEEP=1`, under `target/golden-review/m4_gi_reference`).
fn flip(name: &str, gi: bool) -> f32 {
    let root = concordia_tests::repo_root();
    std::env::set_current_dir(&root).expect("the repository root");
    let golden = root.join("tests/golden");
    let params = gi.then(|| gi_params(name));
    let (image, reference) =
        concordia_tests::reference::render_with(name, None, &golden, "vsm", params).unwrap();
    keep(name, gi, (&reference, &image));
    let score = flip_score(&reference, &image).unwrap();
    println!(
        "{name} gi={gi}: mean {:.4} p99 {:.4}",
        score.mean, score.p99
    );
    score.mean
}

fn keep(name: &str, gi: bool, (reference, image): (&RgbaImage, &RgbaImage)) {
    let tag = if gi { "gi" } else { "nogi" };
    concordia_tests::reference::keep(
        "m4_gi_reference",
        &format!("{name}_{tag}"),
        reference,
        image,
    );
}
#[test]
fn gi_cornell_spell_matches_path_traced_reference() {
    if !has_gpu() {
        return;
    }
    let gi = flip("cornell_spell", true);
    assert!(gi <= 0.10, "cornell_spell: FLIP {gi}");
}

#[test]
fn gi_iso_arena_32_spell_lights_matches_reference() {
    if !has_gpu() {
        return;
    }
    let gi = flip("iso_arena", true);
    assert!(gi <= 0.10, "iso_arena: FLIP {gi}");
}

#[test]
fn gi_improves_flip_over_no_gi_by_40_percent() {
    if !has_gpu() {
        return;
    }
    for name in ["cornell_spell", "iso_arena"] {
        let (on, off) = (flip(name, true), flip(name, false));
        assert!(on <= 0.6 * off, "{name}: GI {on} against no GI {off}");
    }
}

/// Regression goldens (`tests/golden/m4_gi/`, tight): both scenes rendered with GI as their
/// reference tests run it, so a change to probe GI shows here before it shows in FLIP.
#[test]
fn gi_regression_goldens() {
    if !has_gpu() {
        return;
    }
    let root = concordia_tests::repo_root();
    std::env::set_current_dir(&root).expect("the repository root");
    let golden = root.join("tests/golden");
    let store = concordia_testkit::golden::Golden::from_env();
    for name in ["cornell_spell", "iso_arena"] {
        let (image, _) = concordia_tests::reference::render_with(
            name,
            None,
            &golden,
            "vsm",
            Some(gi_params(name)),
        )
        .unwrap();
        let score = store.check("m4_gi", name, &image).unwrap();
        println!("{name}: {score:?}");
    }
}
