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

/// A perf scene's temporal setting reaches the renderer: the own resolve is a timed pass (4.6
/// plan Q13).
#[test]
fn runner_measures_with_the_scenes_temporal_setting() {
    let Ok(caps) = concordia_rhi_metal::probe() else {
        assert!(!concordia_testkit::gpu_required(), "no Metal device");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let scene = perf_folder(dir.path(), &SystemProbe.host(&caps));
    let text = std::fs::read_to_string(&scene).unwrap();
    let text = text.replace(
        "fov_y: 0.9,",
        "fov_y: 0.9, temporal: Some((backend: own, upscale: 2.0)),",
    );
    std::fs::write(&scene, text).unwrap();
    let options = RunOptions {
        warmup_frames: None,
        frames: Some(4),
    };
    let run = run(&scene, &options, &SystemProbe).expect("a run");
    let names: Vec<&str> = run.report.passes.keys().map(String::as_str).collect();
    for pass in ["temporal.resolve", "temporal.keep_depth", "tonemap"] {
        assert!(names.contains(&pass), "{pass} in {names:?}");
    }
}

/// A perf scene's shadow technique reaches the renderer: with `vsm` and a shadowed sun, the page
/// passes are timed (M4 4.7).
#[test]
fn runner_measures_with_the_scenes_shadow_technique() {
    let Ok(caps) = concordia_rhi_metal::probe() else {
        assert!(!concordia_testkit::gpu_required(), "no Metal device");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let scene = perf_folder(dir.path(), &SystemProbe.host(&caps));
    let text = std::fs::read_to_string(&scene).unwrap();
    std::fs::write(
        &scene,
        text.replace("fov_y: 0.9,", "fov_y: 0.9, shadows: Some(vsm),"),
    )
    .unwrap();
    let options = RunOptions {
        warmup_frames: None,
        frames: Some(4),
    };
    let run = run(&scene, &options, &SystemProbe).expect("a run");
    let names: Vec<&str> = run.report.passes.keys().map(String::as_str).collect();
    let vsm = concordia_render::vsm::supported(&caps).is_ok();
    for pass in ["forward.prepass", "vsm.mark", "vsm.allocate", "vsm.render"] {
        assert_eq!(names.contains(&pass), vsm, "{pass} in {names:?}");
    }
}

/// Copies `from` into `to`, leaving out the derived data (`.concordia/`).
fn copy_project(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_name() == ".concordia" {
            continue;
        }
        if src.is_dir() {
            std::fs::create_dir_all(&dst).unwrap();
            copy_project(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// A generated file left out of the project (only its sidecar's recipe is there) is made again
/// before the scene opens (M4 4.4c), so a perf scene can use generated content kept out of git.
#[test]
fn runner_restores_generated_assets_before_opening() {
    let Ok(caps) = concordia_rhi_metal::probe() else {
        assert!(!concordia_testkit::gpu_required(), "no Metal device");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    copy_project(&examples.join("m1-physics-stack"), &project);
    let manifest = project.join("concordia.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, format!("{text}\n[gen]\n")).unwrap();
    let rock = "assets/rock.glb";
    let loaded = concordia_app::Project::load(&project).unwrap();
    let mut engine = concordia_api::Engine::new(loaded.engine_config().unwrap()).unwrap();
    let session = engine.open_session();
    engine
        .call(
            session,
            "gen.run",
            serde_json::json!({"provider": "mesh", "request": {"Box": {"size": [1.0, 1.0, 1.0]}},
                "path": rock, "actor": "tool:test"}),
        )
        .unwrap();
    drop(engine);
    let bytes = std::fs::read(project.join(rock)).unwrap();
    std::fs::remove_file(project.join(rock)).unwrap();
    let perf = dir.path().join("perf");
    let scene = perf_folder(&perf, &SystemProbe.host(&caps));
    let text = std::fs::read_to_string(&scene).unwrap();
    let original = examples.join("m1-physics-stack").canonicalize().unwrap();
    let text = text.replace(
        &original.display().to_string(),
        &project.display().to_string(),
    );
    std::fs::write(&scene, text).unwrap();
    let options = RunOptions {
        warmup_frames: Some(0),
        frames: Some(1),
    };
    run(&scene, &options, &SystemProbe).expect("a run");
    let restored = std::fs::read(project.join(rock)).ok();
    assert!(
        restored == Some(bytes),
        "{rock} was not restored before the run"
    );
}
