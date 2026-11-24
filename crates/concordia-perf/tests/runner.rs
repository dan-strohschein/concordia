//! The runner measures a real scene through the Engine API (M4 phase 4.1a). Needs a GPU (required
//! with CONCORDIA_REQUIRE_GPU=1).

use std::path::Path;

use concordia_perf::machine::SystemProbe;
use concordia_perf::report::Status;
use concordia_perf::{HostInfo, HostProbe, RunOptions, Verdict, run};
use concordia_rhi::RhiCaps;

/// A perf folder in `dir` for the M1 gate scene, whose reference machine is this host, with a
/// generous budget on `forward`.
fn perf_folder(dir: &Path, host: &HostInfo) -> std::path::PathBuf {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let project = examples.join("m1-physics-stack").canonicalize().unwrap();
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("budgets")).unwrap();
    let os_major: u32 = host.os_version.split('.').next().unwrap().parse().unwrap();
    std::fs::write(
        dir.join("reference.ron"),
        format!(
            "ReferenceMachine(name: \"this host\", device_name: {:?}, gpu_cores: {}, memory_gb: {}, os: {:?}, os_major: {os_major}, families: {:?})",
            host.device_name,
            host.gpu_cores.unwrap(),
            host.memory_gb.unwrap(),
            host.os,
            host.families,
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("budgets/smoke.budget.ron"),
        "BudgetFile(scene: \"smoke\", frame: (median_ms: Some(1000.0), p95_ms: None), passes: {\"forward\": (median_ms: Some(1000.0), p95_ms: None)})",
    )
    .unwrap();
    let scene = dir.join("scenes/smoke.perf.ron");
    std::fs::write(
        &scene,
        format!(
            "PerfScene(project: {:?}, scene: \"scenes/stack.scene.ron\", width: 320, height: 180, warmup_frames: 2, frames: 300, fov_y: 0.9, path: [(eye: (7.0, 5.5, 10.0), target: (0.0, 1.0, 0.0)), (eye: (-7.0, 5.5, 10.0), target: (0.0, 1.0, 0.0))])",
            project.display().to_string()
        ),
    )
    .unwrap();
    scene
}

/// Another machine: same GPU name, fewer cores.
struct Elsewhere;

impl HostProbe for Elsewhere {
    fn host(&self, caps: &RhiCaps) -> HostInfo {
        let mut host = SystemProbe.host(caps);
        host.gpu_cores = Some(host.gpu_cores.unwrap_or(1) + 1);
        host
    }
}

#[test]
fn runner_measures_each_pass_and_judges_on_the_reference() {
    let Ok(caps) = concordia_rhi_metal::probe() else {
        assert!(!concordia_testkit::gpu_required(), "no Metal device");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let scene = perf_folder(dir.path(), &SystemProbe.host(&caps));
    let options = RunOptions {
        warmup_frames: None,
        frames: Some(6),
    };
    let here = run(&scene, &options, &SystemProbe).expect("a run");
    assert_eq!(here.report.scene, "smoke");
    assert_eq!(here.report.frames, 6);
    let names: Vec<&str> = here.report.passes.keys().map(String::as_str).collect();
    for pass in ["lights.cluster", "forward", "tonemap", "readback"] {
        assert!(names.contains(&pass), "{pass} in {names:?}");
    }
    assert!(here.report.frame.median_ms > 0.0);
    assert_eq!(here.verdict, Verdict::Pass, "{:?}", here.report);
    assert_eq!(here.report.status, Status::pass);

    let written = here.write(&dir.path().join("out")).expect("writes");
    assert_eq!(written, dir.path().join("out/smoke.perf.ron"));
    let text = std::fs::read_to_string(&written).unwrap();
    let back: concordia_perf::PerfReport = concordia_scene::read_document(&text).unwrap();
    assert_eq!(back, here.report);

    let there = run(&scene, &options, &Elsewhere).expect("a run elsewhere");
    assert!(
        matches!(there.verdict, Verdict::NotReference { .. }),
        "{:?}",
        there.verdict
    );
    assert_eq!(there.report.status, Status::not_reference);
    assert!(
        there.report.notes[0].contains("GPU cores"),
        "{:?}",
        there.report.notes
    );
}
