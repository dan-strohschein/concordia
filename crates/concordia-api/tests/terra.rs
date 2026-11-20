//! Terra templates, spawners and the `terra.*` methods (M2 phase 2.0c): project templates in
//! `templates/**/*.template.ron`, spawners expanded at play start, batch spawns in the play world.

mod common;

use std::sync::Arc;

use common::Harness;
use concordia_api::EngineModule;
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_terra::register_component;
use concordia_terra_sim::{EntityTrait, TemplateBuilder, TemplateError, register_trait};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A shared team id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Team(pub u32);

/// Tag: hostile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Hostile;

/// Trait: a team (shared) and hostility (tag).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Faction {
    /// Team id.
    pub team: u32,
}

impl EntityTrait for Faction {
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        template.add_shared(Team(self.team))?;
        template.add(Hostile)
    }
}

/// Trait that needs a processor no module provides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Thinking {}

impl EntityTrait for Thinking {
    fn build(&self, _: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        Ok(())
    }

    fn requires(&self) -> Vec<String> {
        vec!["ai.think".to_owned()]
    }
}

#[derive(Debug)]
struct Crowds;

impl EngineModule for Crowds {
    fn name(&self) -> &'static str {
        "crowds"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        register_component::<Team>(registry)?;
        register_component::<Hostile>(registry)?;
        register_trait::<Faction>(registry)?;
        register_trait::<Thinking>(registry)
    }
}

const GOBLIN: &str = r#"Template(
    format: 1,
    description: "A small hostile walker.",
    traits: {
        "Faction": (
            team: 2,
        ),
        "Movement": (
            velocity: (1.0, 0.0, 0.0),
        ),
    },
    components: {
        "Name": "Goblin",
    },
)
"#;

const THINKER: &str = r#"Template(
    format: 1,
    description: "Needs an AI processor.",
    traits: {
        "Thinking": (),
    },
    components: {},
)
"#;

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let templates = dir.path().join("templates/enemies");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(templates.join("goblin.template.ron"), GOBLIN).unwrap();
    std::fs::write(templates.join("thinker.template.ron"), THINKER).unwrap();
    Harness::with_config(dir, |c| c.modules = vec![Arc::new(Crowds)]).ready()
}

fn play_count(h: &mut Harness, with: &[&str]) -> usize {
    h.ok(
        "query.run",
        json!({"with": with, "world": "play", "limit": 1000}),
    )["rows"]
        .as_array()
        .unwrap()
        .len()
}

fn edit_count(h: &mut Harness) -> u64 {
    h.ok("scene.info", json!({}))["entity_count"]
        .as_u64()
        .unwrap()
}

#[test]
fn entity_templates_list_builtins_and_project_templates_with_traits() {
    let mut h = harness();
    let list = h.ok("entity.templates", json!({}));
    let goblin = list["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "goblin")
        .unwrap_or_else(|| panic!("{list}"));
    assert_eq!(goblin["description"], "A small hostile walker.");
    assert_eq!(goblin["traits"], json!(["Faction", "Movement"]));
    assert_eq!(
        goblin["components"],
        json!(["Hostile", "Name", "Team", "Transform", "Velocity"])
    );
    let empty = list["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "Empty")
        .unwrap();
    assert_eq!(empty["traits"], json!([]));
}

#[test]
fn entity_create_spawns_a_project_template_into_the_edit_world() {
    let mut h = harness();
    let id = h.ok("entity.create", json!({"template": "goblin"}))["id"].clone();
    let team = h.ok("component.get", json!({"entity": id, "component": "Team"}));
    assert_eq!(team["value"], json!(2));
    assert_eq!(edit_count(&mut h), 1);
    h.ok("txn.undo", json!({}));
    assert_eq!(edit_count(&mut h), 0, "one undo step");
}

#[test]
fn spawners_expand_when_play_starts() {
    let mut h = harness();
    let id = h.ok("entity.create", json!({"template": "Empty"}))["id"].clone();
    h.ok(
        "component.set",
        json!({"entity": id, "component": "Spawner", "value": {
            "template": "goblin", "count": 40, "seed": 5,
            "distribution": {"Disc": {"radius": 8.0}}}}),
    );
    assert_eq!(edit_count(&mut h), 1);
    h.ok("sim.step", json!({"ticks": 1}));
    assert_eq!(play_count(&mut h, &["Team"]), 40);
    assert_eq!(
        edit_count(&mut h),
        1,
        "the edit world keeps only the spawner"
    );
}

#[test]
fn spawn_batch_and_despawn_batch_act_on_the_play_world() {
    let mut h = harness();
    let spawned = h.ok(
        "terra.spawn_batch",
        json!({"template": "goblin", "count": 100, "seed": 3,
               "distribution": {"Grid": {"spacing": 1.5}}}),
    );
    assert_eq!(spawned["count"], 100);
    let ids = spawned["entities"].as_array().unwrap().clone();
    assert_eq!(ids.len(), 100);
    assert_eq!(edit_count(&mut h), 0);
    assert_eq!(play_count(&mut h, &["Team"]), 100);
    let gone = h.ok(
        "terra.despawn_batch",
        json!({"entities": ids[..10].to_vec()}),
    );
    assert_eq!(gone["count"], 10);
    assert_eq!(play_count(&mut h, &["Team"]), 90);
    let gone = h.ok("terra.despawn_batch", json!({"with": ["Hostile"]}));
    assert_eq!(gone["count"], 90);
    assert_eq!(play_count(&mut h, &["Team"]), 0);
}

#[test]
fn spawn_batch_validates_and_names_missing_processors() {
    let mut h = harness();
    let err = h.err(
        "terra.spawn_batch",
        json!({"template": "goblin", "count": 0}),
    );
    assert!(err.to_string().contains("count"), "{err}");
    let err = h.err(
        "terra.spawn_batch",
        json!({"template": "gobln", "count": 1}),
    );
    assert!(
        err.to_string().contains("goblin"),
        "suggests the name: {err}"
    );
    let err = h.err(
        "terra.spawn_batch",
        json!({"template": "thinker", "count": 1}),
    );
    assert!(err.to_string().contains("ai.think"), "{err}");
}

#[test]
fn terra_stats_and_inspect_report_storage() {
    let mut h = harness();
    let spawned = h.ok(
        "terra.spawn_batch",
        json!({"template": "goblin", "count": 5, "seed": 1}),
    );
    let stats = h.ok("terra.stats", json!({"world": "play"}));
    assert_eq!(stats["entities"], 5);
    assert!(stats["archetypes"].as_u64().unwrap() >= 2, "{stats}");
    assert!(
        stats["shared_values"].as_u64().unwrap() >= 2,
        "Team and the tag: {stats}"
    );
    assert_eq!(stats["skipped_commands"], 0);
    let phases = stats["phases"].as_array().unwrap();
    let post = phases.iter().find(|p| p["phase"] == "PostPhysics").unwrap();
    assert!(
        post["waves"].to_string().contains("integrate_velocity"),
        "{stats}"
    );
    let id = &spawned["entities"][0];
    let info = h.ok("terra.inspect", json!({"entity": id, "world": "play"}));
    let storage = |name: &str| {
        info["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .map(|c| c["storage"].clone())
            .unwrap_or(Value::Null)
    };
    assert_eq!(storage("Team"), json!("shared"));
    assert_eq!(storage("Hostile"), json!("tag"));
    assert_eq!(storage("Velocity"), json!("dense"));
    assert_eq!(info["chunk"], 0);
    let edit = h.ok("terra.stats", json!({}));
    assert_eq!(edit["entities"], 0);
    assert_eq!(edit["phases"], json!([]), "the edit world has no schedule");
}
