//! Templates and traits (M2 phase 2.0c): Mass-style traits (Rust types with reflected
//! parameters) composed by `.template.ron` files. Concepts from public docs only.

mod common;

use std::collections::BTreeMap;

use common::{GOBLIN, Hostile, Team, registry};
use concordia_core::id::{DeterministicIdGen, IdSource};
use concordia_core::math::Vec3;
use concordia_scene::builtin::{Name, Transform, Velocity};
use concordia_scene::{read_template, write_template};
use concordia_terra::{StorageKind, World};
use concordia_terra_sim::{Template, TemplateError};
use serde_json::json;

fn goblin() -> Template {
    let doc = read_template(GOBLIN, &registry()).unwrap();
    Template::from_doc("goblin", &doc).unwrap()
}

#[test]
fn template_composes_traits_into_an_archetype() {
    let registry = registry();
    let compiled = goblin().compile(&registry).unwrap();
    assert_eq!(
        compiled.component_names(),
        ["Hostile", "Name", "Team", "Transform", "Velocity"]
    );
    assert_eq!(compiled.requires(), ["integrate_velocity"]);
    let mut world = World::new();
    let id = DeterministicIdGen::new(4).next_id().unwrap();
    compiled.spawn(&mut world, id).unwrap();
    assert_eq!(world.get::<Name>(id), Some(&Name("Goblin".into())));
    assert_eq!(
        world.get::<Velocity>(id).unwrap().linear,
        Vec3::new(1.5, 0.0, 0.0)
    );
    assert_eq!(world.get::<Team>(id), Some(&Team(2)));
    assert_eq!(world.get::<Hostile>(id), Some(&Hostile));
    assert_eq!(world.storage_kind::<Team>(), Some(StorageKind::Shared));
    assert_eq!(world.storage_kind::<Hostile>(), Some(StorageKind::Tag));
    let json = compiled.components_json();
    assert_eq!(json["Name"], json!("Goblin"));
}

#[test]
fn component_values_override_trait_defaults() {
    let mut template = goblin();
    template.components.insert(
        "Velocity".into(),
        json!({"linear": [0.0, 0.0, 9.0], "angular": [0.0, 0.0, 0.0]}),
    );
    let compiled = template.compile(&registry()).unwrap();
    assert_eq!(
        compiled.components_json()["Velocity"]["linear"],
        json!([0.0, 0.0, 9.0])
    );
}

#[test]
fn trait_conflicts_are_reported_with_both_traits() {
    let mut template = goblin();
    template.traits.insert("Drifting".into(), json!({}));
    match template.compile(&registry()) {
        Err(TemplateError::TraitConflict {
            component,
            first,
            second,
        }) => {
            assert_eq!(component, "Velocity");
            assert_eq!(
                [first.as_str(), second.as_str()],
                ["Drifting", "Moving"],
                "both traits, in name order"
            );
        }
        other => panic!("expected a trait conflict, got {other:?}"),
    }
}

#[test]
fn unknown_traits_and_bad_parameters_are_errors_with_hints() {
    let registry = registry();
    let mut template = goblin();
    template
        .traits
        .insert("Movin".into(), json!({"speed": 1.0}));
    match template.compile(&registry) {
        Err(TemplateError::UnknownTrait { name, did_you_mean }) => {
            assert_eq!(name, "Movin");
            assert_eq!(did_you_mean.as_deref(), Some("Moving"));
        }
        other => panic!("expected an unknown trait, got {other:?}"),
    }
    let mut template = goblin();
    template.traits.insert("Name".into(), json!("x"));
    assert!(matches!(
        template.compile(&registry),
        Err(TemplateError::NotATrait { .. })
    ));
    let mut template = goblin();
    template
        .traits
        .insert("Moving".into(), json!({"speed": 500.0}));
    assert!(matches!(
        template.compile(&registry),
        Err(TemplateError::InvalidValue { .. })
    ));
}

#[test]
fn template_roundtrips_byte_identical() {
    let registry = registry();
    let canonical = write_template(&read_template(GOBLIN, &registry).unwrap()).unwrap();
    let again = write_template(&read_template(&canonical, &registry).unwrap()).unwrap();
    assert_eq!(again, canonical);
    let doc = goblin().to_doc(&registry).unwrap();
    assert_eq!(
        write_template(&doc).unwrap(),
        canonical,
        "JSON round trip too"
    );
    assert!(canonical.contains("\"Moving\": (\n"), "{canonical}");
}

#[test]
fn template_schemas_come_from_reflection() {
    let registry = registry();
    let schema = concordia_terra_sim::trait_schema(&registry, "Moving").unwrap();
    assert_eq!(schema["properties"]["speed"]["maximum"], json!(100.0));
    let traits: Vec<&str> = concordia_terra_sim::trait_names(&registry).collect();
    assert_eq!(
        traits,
        ["Drifting", "Faction", "Movement", "Moving"],
        "with the built-in Movement"
    );
}

#[test]
fn batch_spawn_of_100k_from_template_is_deterministic() {
    let run = || {
        let registry = registry();
        let compiled = goblin().compile(&registry).unwrap();
        let mut world = World::new();
        let mut ids = DeterministicIdGen::new(11);
        let spawned = compiled
            .spawn_batch(&mut world, &mut ids, 100_000, |i| {
                Some(Vec3::new(i as f32, 0.0, 0.0))
            })
            .unwrap();
        assert_eq!(spawned.len(), 100_000);
        assert_eq!(
            world.get::<Transform>(spawned[7]).unwrap().translation,
            Vec3::new(7.0, 0.0, 0.0)
        );
        world.validate_storage().unwrap();
        world.state_hash().unwrap()
    };
    assert_eq!(run(), run());
}

#[test]
fn missing_processors_are_named() {
    let compiled = goblin().compile(&registry()).unwrap();
    assert_eq!(
        compiled.missing_processors(["physics.step"]),
        ["integrate_velocity"]
    );
    assert!(
        compiled
            .missing_processors(["integrate_velocity"])
            .is_empty()
    );
}

#[test]
fn builtin_templates_are_plain_component_maps() {
    let cube = Template::new(
        "Empty",
        "An entity with a name.",
        [("Name", json!("Empty"))],
    );
    assert!(cube.traits.is_empty());
    let compiled = cube.compile(&registry()).unwrap();
    assert_eq!(compiled.component_names(), ["Name"]);
    let unknown = Template::new("Bad", "", [("Nmae", json!("x"))]);
    assert!(matches!(
        unknown.compile(&registry()),
        Err(TemplateError::UnknownComponent { .. })
    ));
    let _: BTreeMap<String, serde_json::Value> = compiled.components_json();
}

#[test]
fn concordia_fmt_formats_templates_canonically() {
    let registry = registry();
    let canonical = write_template(&read_template(GOBLIN, &registry).unwrap()).unwrap();
    assert_eq!(
        concordia_scene::format(GOBLIN, &registry).unwrap(),
        canonical
    );
    assert_eq!(
        concordia_scene::format(&canonical, &registry).unwrap(),
        canonical
    );
}
