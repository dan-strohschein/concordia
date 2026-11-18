//! M0 gate layers 2 and 3: outcome checks for a real agent session (run via `just gate-live M0`).
//!
//! These read a project the agent worked on (`M0_LIVE_PROJECT`) and, for layer 2, the
//! `claude -p --output-format json` result (`M0_LIVE_RESULT`). They assert outcomes, never transcripts.

use std::path::{Path, PathBuf};

use concordia_reflect::TypeRegistry;
use concordia_scene::builtin::{Name, Transform};
use concordia_scene::{SceneDoc, read_scene};
use serde_json::Value;

const EPSILON: f32 = 1e-4;

fn env_path(var: &str) -> PathBuf {
    let value =
        std::env::var_os(var).unwrap_or_else(|| panic!("set {var} (run via `just gate-live M0`)"));
    PathBuf::from(value)
}

fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    registry
}

/// Reads a scene, asserting it parses and is canonical.
fn canonical_scene(path: &Path) -> SceneDoc {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let registry = registry();
    assert_eq!(
        concordia_scene::format(&text, &registry).unwrap(),
        text,
        "{} is canonical",
        path.display()
    );
    read_scene(&text, &registry).unwrap()
}

fn journal(project: &Path, scene: &SceneDoc) -> Vec<Value> {
    let path = project
        .join(".concordia/journal")
        .join(format!("{}.cmdlog.jsonl", scene.id));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn by_name<'a>(scene: &'a SceneDoc, name: &str) -> &'a concordia_scene::EntityDoc {
    let matches: Vec<_> = scene
        .entities
        .iter()
        .filter(|e| {
            e.components
                .get("Name")
                .and_then(|n| n.as_any().downcast_ref::<Name>())
                .is_some_and(|n| n.0 == name)
        })
        .collect();
    assert_eq!(matches.len(), 1, "exactly one entity named {name:?}");
    matches[0]
}

fn transform(entity: &concordia_scene::EntityDoc) -> Transform {
    *entity
        .components
        .get("Transform")
        .and_then(|t| t.as_any().downcast_ref::<Transform>())
        .expect("has a Transform")
}

fn close(actual: [f32; 3], expected: [f32; 3], what: &str) {
    let ok = actual
        .iter()
        .zip(expected)
        .all(|(a, e)| (a - e).abs() <= EPSILON);
    assert!(ok, "{what}: {actual:?} is not {expected:?}");
}

/// Commits that spawned an entity with `Name == name`.
fn spawn_txns(entries: &[Value], name: &str) -> Vec<Value> {
    entries
        .iter()
        .filter(|e| e["kind"] == "commit")
        .filter(|e| {
            e["ops"].as_array().is_some_and(|ops| {
                ops.iter()
                    .any(|op| op["op"]["Spawn"]["components"]["Name"] == name)
            })
        })
        .map(|e| e["txn"].clone())
        .collect()
}

#[test]
#[ignore = "run via just gate-live M0"]
fn m0_live_outcome_check() {
    let project = env_path("M0_LIVE_PROJECT");
    let result: Value =
        serde_json::from_str(&std::fs::read_to_string(env_path("M0_LIVE_RESULT")).unwrap())
            .unwrap();
    assert_eq!(result["is_error"], false, "the agent run failed: {result}");
    let text = result["result"].as_str().unwrap_or("");
    let line = text
        .lines()
        .rev()
        .find(|l| l.contains("RESULT:"))
        .unwrap_or_else(|| panic!("no RESULT: line in {text:?}"));
    for tree in ["Tree 1", "Tree 2", "Tree 3"] {
        assert!(line.contains(tree), "{line:?} names {tree}");
    }
    assert!(!line.contains("Lamp"), "{line:?}: the Lamp is not at z > 1");

    let scene = canonical_scene(&project.join("scenes/courtyard.scene.ron"));
    assert_eq!(scene.name, "Courtyard");
    assert_eq!(scene.entities.len(), 4, "Lamp and three trees, no Rock");
    let lamp = transform(by_name(&scene, "Lamp"));
    close(
        lamp.translation.to_array(),
        [2.0, 0.0, -3.0],
        "Lamp translation",
    );
    close(lamp.scale.to_array(), [1.0, 2.0, 1.0], "Lamp scale");
    for (i, x) in [-4.0, 0.0, 4.0].into_iter().enumerate() {
        let tree = transform(by_name(&scene, &format!("Tree {}", i + 1)));
        let at = [tree.translation.x, tree.translation.y, tree.translation.z];
        close(at, [x, 0.0, 5.0], &format!("Tree {} translation", i + 1));
    }

    let entries = journal(&project, &scene);
    let trees = entries
        .iter()
        .find(|e| e["kind"] == "commit" && e["label"] == "plant trees")
        .expect("a 'plant trees' commit");
    let spawns = trees["ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|op| op["op"].get("Spawn").is_some())
        .count();
    assert_eq!(spawns, 3, "plant trees spawns three entities");
    let rock = spawn_txns(&entries, "Rock");
    assert!(
        entries
            .iter()
            .any(|e| e["kind"] == "undo" && rock.contains(&e["target"])),
        "the Rock spawn was undone"
    );
    for entry in entries
        .iter()
        .filter(|e| e["kind"] == "commit" || e["kind"] == "undo")
    {
        assert!(
            entry["actor"]
                .as_str()
                .is_some_and(|a| a.starts_with("agent:")),
            "{entry}"
        );
    }
}

#[test]
#[ignore = "run manually after the in-session check (gate layer 3)"]
fn m0_scene_is_valid() {
    let project = env_path("M0_LIVE_PROJECT");
    let scenes: Vec<PathBuf> = std::fs::read_dir(project.join("scenes"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".scene.ron"))
        .collect();
    assert!(!scenes.is_empty(), "no scenes in {}", project.display());
    let mut attributed = false;
    for path in scenes {
        let scene = canonical_scene(&path);
        let Ok(text) = std::fs::read_to_string(
            project
                .join(".concordia/journal")
                .join(format!("{}.cmdlog.jsonl", scene.id)),
        ) else {
            continue;
        };
        attributed |= text
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .any(|e| {
                e["kind"] == "commit"
                    && e["actor"].as_str().is_some_and(|a| a.starts_with("agent:"))
            });
    }
    assert!(
        attributed,
        "at least one scene journal has a commit by an agent: actor"
    );
}
