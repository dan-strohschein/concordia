//! `concordia gpu-probe` (M1 phase 1.2a).

mod common;

use common::{run, stderr, stdout};
use serde_json::Value;

#[test]
fn gpu_probe_prints_a_json_report_and_require_m1_matches_it() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(&["gpu-probe"], dir.path());
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let report: Value = serde_json::from_str(&stdout(&output)).unwrap();
    let auto = concordia_rhi_select::choose_from_env(Default::default()).unwrap();
    assert_eq!(
        report["backend"],
        auto.as_str(),
        "the platform's (or CONCORDIA_BACKEND's)"
    );
    let meets_m1 = match report["available"].as_bool() {
        Some(true) => {
            assert_eq!(report["caps"]["backend"], report["backend"], "{report}");
            report["missing_for_m1"].as_array().unwrap().is_empty()
        }
        Some(false) => {
            assert!(
                report["reason"].as_str().is_some_and(|r| !r.is_empty()),
                "{report}"
            );
            false
        }
        None => panic!("no `available` field: {report}"),
    };
    let required = run(&["gpu-probe", "--require-m1"], dir.path());
    assert_eq!(required.status.success(), meets_m1, "{}", stderr(&required));
    if !meets_m1 {
        assert!(stderr(&required).contains("M1"), "{}", stderr(&required));
    }
}

#[test]
fn gpu_probe_record_prints_the_raw_probe_record() {
    let dir = tempfile::tempdir().unwrap();
    for backend in concordia_rhi::Backend::ALL {
        let output = run(
            &["gpu-probe", "--record", "--backend", backend.as_str()],
            dir.path(),
        );
        if !output.status.success() {
            assert!(
                !concordia_testkit::device_required(backend),
                "{backend}: {}",
                stderr(&output)
            );
            assert!(stderr(&output).contains("no"), "{}", stderr(&output));
            continue;
        }
        let record: Value = serde_json::from_str(&stdout(&output)).expect("a probe record");
        assert!(
            record["device_name"]
                .as_str()
                .is_some_and(|n| !n.is_empty()),
            "{record}"
        );
        if backend == concordia_rhi::Backend::Metal {
            let record: concordia_rhi_metal::record::ProbeRecord =
                serde_json::from_value(record).unwrap();
            assert!(record.families.windows(2).all(|w| w[0] < w[1]), "sorted");
        }
    }
}

/// `--backend metal|vulkan` probes that backend; without its device the report says why
/// (Vulkan on a Mac without a loader) and the command still succeeds.
#[test]
fn gpu_probe_backend_picks_the_device_and_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    for backend in concordia_rhi::Backend::ALL {
        let output = run(&["gpu-probe", "--backend", backend.as_str()], dir.path());
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let report: Value = serde_json::from_str(&stdout(&output)).unwrap();
        assert_eq!(report["backend"], backend.as_str(), "{report}");
        match report["available"].as_bool() {
            Some(true) => assert_eq!(report["caps"]["backend"], backend.as_str(), "{report}"),
            Some(false) => {
                assert!(!concordia_testkit::device_required(backend), "{report}");
                assert!(
                    report["reason"].as_str().is_some_and(|r| !r.is_empty()),
                    "{report}"
                );
            }
            None => panic!("no `available` field: {report}"),
        }
    }
    let bad = run(&["gpu-probe", "--backend", "dx12"], dir.path());
    assert_eq!(bad.status.code(), Some(2), "a usage error");
    assert!(stderr(&bad).contains("vulkan"), "{}", stderr(&bad));
}
