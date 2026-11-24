//! The repository's perf files (`tests/perf/`) are valid and canonical, and every budget names a
//! perf scene (M4 phase 4.1a).

use std::path::{Path, PathBuf};

use concordia_perf::{BudgetFile, PerfScene, ReferenceMachine};

fn perf() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/perf")
}

fn files(dir: &str, suffix: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = std::fs::read_dir(perf().join(dir))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.strip_suffix(suffix)?.to_owned();
            Some((name, std::fs::read_to_string(&p).unwrap()))
        })
        .collect();
    found.sort();
    found
}

#[test]
fn the_reference_machine_is_the_authors_m3_pro() {
    let text = std::fs::read_to_string(perf().join("reference.ron")).unwrap();
    let reference = ReferenceMachine::read(&text).expect("valid");
    assert_eq!(
        (reference.device_name.as_str(), reference.gpu_cores),
        ("Apple M3 Pro", 18)
    );
    assert_eq!(
        concordia_scene::write_document(&reference).unwrap(),
        text,
        "canonical"
    );
}

#[test]
fn perf_scenes_are_valid_canonical_and_their_projects_exist() {
    let scenes = files("scenes", ".perf.ron");
    assert!(scenes.len() >= 2, "{scenes:?}");
    for (name, text) in &scenes {
        let scene = PerfScene::read(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            &concordia_scene::write_document(&scene).unwrap(),
            text,
            "{name}: canonical"
        );
        let project = perf().join("scenes").join(&scene.project);
        assert!(
            project.join(&scene.scene).is_file(),
            "{name}: {}",
            scene.scene
        );
    }
}

#[test]
fn every_budget_names_a_perf_scene() {
    let scenes: Vec<String> = files("scenes", ".perf.ron")
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    for (name, text) in files("budgets", ".budget.ron") {
        let budget = BudgetFile::read(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(budget.scene, name, "the file is named after its scene");
        assert!(scenes.contains(&name), "{name} has no perf scene");
        assert_eq!(budget.to_text().unwrap(), text, "{name}: canonical");
    }
}
