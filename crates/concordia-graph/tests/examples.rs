//! The example graphs (M4 4.9's done criterion): ten that check clean and are canonical text, ten
//! broken ones whose diagnostics are snapshotted.

use std::path::{Path, PathBuf};

use concordia_core::math::Vec3;
use concordia_graph::{NodeLibrary, check, read_graph, write_graph};
use concordia_reflect::{Reflect, TypeRegistry};
use serde::{Deserialize, Serialize};

/// Hit points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
struct Health {
    /// Current.
    value: f32,
    /// Most.
    max: f32,
}

/// Movement.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
struct Mover {
    /// Metres per second.
    velocity: Vec3,
}

/// Signal: took damage.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal)]
struct Damaged {
    /// How much.
    amount: f32,
}

fn library() -> NodeLibrary {
    let mut reg = TypeRegistry::new();
    reg.register::<Health>().unwrap();
    reg.register::<Mover>().unwrap();
    reg.register::<Damaged>().unwrap();
    NodeLibrary::build(&reg, |r| {
        ["Health", "Mover", "Damaged"].contains(&r.info().name)
    })
}

fn graphs(folder: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/graphs")
        .join(folder);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".graph.ron"))
        .collect();
    files.sort();
    files
}

fn name(path: &Path) -> String {
    path.file_name()
        .unwrap()
        .to_string_lossy()
        .trim_end_matches(".graph.ron")
        .to_owned()
}

#[test]
fn ten_example_graphs_check_clean_and_are_canonical() {
    let lib = library();
    let files = graphs("ok");
    assert_eq!(files.len(), 10);
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let graph = read_graph(&text).unwrap_or_else(|e| panic!("{}: {e}", name(&path)));
        let report = check(&graph, &lib);
        assert!(report.is_ok(), "{}: {:#?}", name(&path), report.diagnostics);
        let canonical = write_graph(&graph).unwrap();
        if std::env::var_os("CONCORDIA_GRAPH_CANONICALIZE").is_some() {
            std::fs::write(&path, &canonical).unwrap();
            continue;
        }
        assert_eq!(
            text,
            canonical,
            "{} isn't canonical (CONCORDIA_GRAPH_CANONICALIZE=1 rewrites)",
            name(&path)
        );
    }
}

#[test]
fn ten_broken_graphs_report_the_expected_diagnostics() {
    let lib = library();
    let files = graphs("broken");
    assert_eq!(files.len(), 10);
    let mut all = serde_json::Map::new();
    for path in files {
        let graph = read_graph(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let diagnostics = check(&graph, &lib).diagnostics;
        assert!(!diagnostics.is_empty(), "{} checks clean", name(&path));
        all.insert(name(&path), serde_json::to_value(&diagnostics).unwrap());
    }
    insta::assert_json_snapshot!("broken_graph_diagnostics", all);
}
