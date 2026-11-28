//! The M4 visual showcase on Lamplight (M4 4.13c, N7): the harbour watch at night with every M4
//! technique on (meshlets with GPU culling, virtual shadow maps, probe GI, TAA), from the two
//! Lamplight reference views. Regression goldens in `m4_showcase`; the FLIP against each view's
//! path-traced reference and the frame times are reported, not gated (perf deferred by the author).

use concordia_testkit::golden::{Golden, flip_score};
use serde_json::json;

const VIEWS: [&str; 2] = ["lamplight_town", "lamplight_street"];

#[test]
fn lamplight_showcase_renders_with_every_m4_technique() {
    let Ok(backend) = concordia_rhi_select::choose_from_env(Default::default()) else {
        return;
    };
    let golden = concordia_tests::repo_root().join("tests/golden");
    let mut missed = Vec::new();
    for name in VIEWS {
        let (mut file, reference) = concordia_tests::reference::reference_of(name, &golden)
            .unwrap_or_else(|e| panic!("{e}"));
        file.project = concordia_tests::repo_root()
            .join(&file.project)
            .display()
            .to_string();
        file.scene = Some("scenes/harbour_watch.scene.ron".into());
        let calls = [
            ("render.set_shadows", json!({"technique": "vsm"})),
            ("render.set_gi", json!({"technique": "probes"})),
            ("render.set_temporal", json!({"backend": "own"})),
        ];
        let (image, results) = concordia_tests::reference::render_configured(&file, &calls)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(results[0]["technique"], "vsm", "{name}: {results:?}");
        assert_eq!(results[1]["technique"], "probes", "{name}: {results:?}");
        assert_eq!(results[2]["backend"], "own", "{name}: {results:?}");
        let flip = flip_score(&reference, &image).unwrap();
        println!(
            "showcase {name}: FLIP against the path-traced reference mean {:.4}, p99 {:.4}",
            flip.mean, flip.p99
        );
        concordia_tests::reference::keep("m4_showcase", name, &reference, &image);
        if let Err(e) = Golden::from_env().check_on(Some(backend), "m4_showcase", name, &image) {
            missed.push(format!("{name}: {e}"));
        }
    }
    assert!(missed.is_empty(), "{}", missed.join("\n"));
}
