//! `concordia reference bless` (M4 4.1b4, ADR-0090): traces a reference twice (two seeds), renders
//! the same view with the renderer, records both FLIPs and writes the image and its file; refused
//! in CI and for noisy references. Needs a GPU for the renderer (required with
//! CONCORDIA_REQUIRE_GPU=1).

mod common;

use std::path::Path;
use std::process::Output;

use common::{concordia, stderr, stdout};
use concordia_testkit::golden::Golden;

/// A golden store in `dir` with one reference of the M1 stack, tolerating `mean` FLIP.
fn store(dir: &Path, mean: f32) -> std::path::PathBuf {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/m1-physics-stack")
        .canonicalize()
        .unwrap();
    let suite = dir.join("golden/m4_reference");
    std::fs::create_dir_all(&suite).unwrap();
    std::fs::write(
        suite.join("tolerances.ron"),
        format!("Tolerances(default: (mean: {mean}, p99: 1.0), goldens: {{}})"),
    )
    .unwrap();
    std::fs::write(
        suite.join("stack.reference.ron"),
        format!(
            "ReferenceFile(project: {:?}, view: Some((eye: (7.0, 5.5, 10.0), target: (0.0, 1.0, 0.0), fov_y_degrees: 50.0)), width: 48, height: 27, spp: 64, seed: 1, max_bounces: 4)",
            project.display().to_string()
        ),
    )
    .unwrap();
    dir.join("golden")
}

/// Runs `concordia reference bless stack` against `golden`; `ci` sets CI=true (otherwise CI is
/// removed: these stores are temporary, never the repository's).
fn bless(golden: &Path, ci: bool) -> Output {
    let mut cmd = concordia();
    cmd.args(["reference", "bless", "stack", "--golden"])
        .arg(golden);
    if ci {
        cmd.env("CI", "true");
    } else {
        cmd.env_remove("CI");
    }
    cmd.output().expect("concordia runs")
}

fn has_gpu() -> bool {
    let ok = concordia_rhi_metal::probe().is_ok();
    assert!(
        ok || !concordia_testkit::gpu_required(),
        "CONCORDIA_REQUIRE_GPU=1 but no GPU"
    );
    ok
}

#[test]
fn blessing_records_the_baseline_and_the_noise() {
    if !has_gpu() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let golden = store(dir.path(), 1.0);
    let out = bless(&golden, false);
    assert!(out.status.success(), "{}{}", stdout(&out), stderr(&out));
    assert!(golden.join("m4_reference/stack.png").is_file());
    let store = Golden::new(golden.clone(), dir.path().join("failures"), false, false);
    let file = store.reference("stack").unwrap();
    let blessed = file.blessed.expect("blessed");
    assert!(
        ["gpu", "cpu"].contains(&blessed.backend.as_str()),
        "{}",
        blessed.backend
    );
    assert!(!blessed.git.is_empty());
    assert!(
        blessed.baseline.mean > 0.0 && blessed.baseline.mean < 1.0,
        "{:?}",
        blessed.baseline
    );
    assert!(
        blessed.noise.mean > 0.0,
        "two seeds differ: {:?}",
        blessed.noise
    );
    assert!(stdout(&out).contains("baseline"), "{}", stdout(&out));
}

#[test]
fn blessing_is_refused_in_ci_before_tracing() {
    let dir = tempfile::tempdir().unwrap();
    let golden = store(dir.path(), 1.0);
    let out = bless(&golden, true);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("CI"), "{}", stderr(&out));
    assert!(!golden.join("m4_reference/stack.png").exists());
}

#[test]
fn a_noisy_reference_is_refused() {
    if !has_gpu() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let golden = store(dir.path(), 0.0001);
    let out = bless(&golden, false);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("spp"), "{}", stderr(&out));
    assert!(!golden.join("m4_reference/stack.png").exists());
}
