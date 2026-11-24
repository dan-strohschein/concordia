//! `concordia perf run` (M4 phase 4.1a): measures a perf scene, writes its report and exits by
//! its verdict. Needs a GPU (required with CONCORDIA_REQUIRE_GPU=1).

mod common;

use std::path::Path;

use common::{run, stderr, stdout};

/// A perf folder in `dir` measuring the M1 gate scene, with `reference` as the reference machine
/// and `budget` (if any) as its budget.
fn perf_folder(dir: &Path, reference: &str, budget: Option<&str>) -> String {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/m1-physics-stack")
        .canonicalize()
        .unwrap();
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("budgets")).unwrap();
    std::fs::write(dir.join("reference.ron"), reference).unwrap();
    if let Some(budget) = budget {
        std::fs::write(dir.join("budgets/gate.budget.ron"), budget).unwrap();
    }
    let scene = dir.join("scenes/gate.perf.ron");
    std::fs::write(
        &scene,
        format!(
            "PerfScene(project: {:?}, scene: \"scenes/stack.scene.ron\", width: 320, height: 180, warmup_frames: 120, frames: 300, fov_y: 0.9, path: [(eye: (7.0, 5.5, 10.0), target: (0.0, 1.0, 0.0))])",
            project.display().to_string()
        ),
    )
    .unwrap();
    scene.display().to_string()
}

const ELSEWHERE: &str = "ReferenceMachine(name: \"elsewhere\", device_name: \"No Such GPU\", gpu_cores: 1, memory_gb: 1, os: \"macOS\", os_major: 1, families: [])";

fn has_gpu() -> bool {
    match concordia_rhi_metal::probe() {
        Ok(_) => true,
        Err(e) => {
            assert!(!concordia_testkit::gpu_required(), "no Metal device: {e}");
            false
        }
    }
}

#[test]
fn perf_run_writes_a_report_and_exits_by_its_verdict() {
    if !has_gpu() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let scene = perf_folder(dir.path(), ELSEWHERE, None);
    let quick = ["--warmup", "1", "--frames", "3", "--out"];
    let mut args = vec!["perf", "run", &scene];
    args.extend(quick);
    let out_arg = out.display().to_string();
    args.push(&out_arg);

    let output = run(&args, dir.path());
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("not_reference"), "{text}");
    assert!(text.contains("forward"), "per-pass lines: {text}");
    let report = std::fs::read_to_string(out.join("gate.perf.ron")).expect("the report");
    assert!(report.contains("status: not_reference"), "{report}");

    let mut strict = args.clone();
    strict.push("--require-reference");
    let output = run(&strict, dir.path());
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("No Such GPU"),
        "{}",
        stderr(&output)
    );

    let host = run(&["gpu-probe"], dir.path());
    let caps: serde_json::Value = serde_json::from_str(&stdout(&host)).unwrap();
    let reference = this_mac(&caps["caps"]);
    let tight = "BudgetFile(scene: \"gate\", frame: (median_ms: Some(1000.0), p95_ms: None), passes: {\"forward\": (median_ms: Some(0.0001), p95_ms: None)})";
    let scene = perf_folder(dir.path(), &reference, Some(tight));
    let mut over = vec!["perf", "run", &scene];
    over.extend(quick);
    over.push(&out_arg);
    let output = run(&over, dir.path());
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("forward: median"),
        "{}",
        stderr(&output)
    );
}

/// A reference file describing this Mac (the same tools `concordia perf run` reads).
fn this_mac(caps: &serde_json::Value) -> String {
    let tool = |p: &str, a: &[&str]| {
        String::from_utf8(
            std::process::Command::new(p)
                .args(a)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
    };
    let cores =
        concordia_perf::machine::parse_gpu_cores(&tool("ioreg", &["-rc", "AGXAccelerator", "-d1"]))
            .unwrap();
    let memory =
        concordia_perf::machine::parse_memory_gb(&tool("sysctl", &["-n", "hw.memsize"])).unwrap();
    let os = tool("sw_vers", &["-productVersion"]);
    format!(
        "ReferenceMachine(name: \"this mac\", device_name: {}, gpu_cores: {cores}, memory_gb: {memory}, os: \"macOS\", os_major: {}, families: {})",
        caps["device_name"],
        os.trim().split('.').next().unwrap(),
        caps["feature_sets"],
    )
}
