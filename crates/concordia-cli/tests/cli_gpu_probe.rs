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
    let meets_m1 = match report["available"].as_bool() {
        Some(true) => {
            assert_eq!(report["caps"]["backend"], "metal", "{report}");
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
    let output = run(&["gpu-probe", "--record"], dir.path());
    if !output.status.success() {
        assert!(!concordia_testkit::gpu_required(), "{}", stderr(&output));
        assert!(stderr(&output).contains("no"), "{}", stderr(&output));
        return;
    }
    let record: concordia_rhi_metal::record::ProbeRecord =
        serde_json::from_str(&stdout(&output)).expect("a probe record");
    assert!(!record.device_name.is_empty());
    assert!(record.families.windows(2).all(|w| w[0] < w[1]), "sorted");
}
