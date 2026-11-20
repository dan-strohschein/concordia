//! Script-defined traits and deferred template spawns (M2 phase 2.6b, ADR-0062).

use concordia_core::id::DeterministicIdGen;
use concordia_reflect::{TypeDesc, TypeRegistry};
use concordia_terra::{CommandBuffer, World, register_dynamic_component};
use concordia_terra_sim::{
    DynamicTrait, Template, TemplateError, register_dynamic_trait, trait_names,
};
use serde_json::{Value, json};

fn desc(name: &str, shape: Value, default: Option<Value>) -> TypeDesc {
    serde_json::from_value(json!({
        "name": name, "docs": name, "shape": shape, "default": default,
    }))
    .unwrap()
}

/// `Hp { value: u32 }` (default 10) and `Wings` (a tag, default unit); `NoDefault { n: u32 }`.
fn registry() -> TypeRegistry {
    let mut reg = TypeRegistry::default();
    concordia_terra_sim::register(&mut reg).unwrap();
    let field = json!({"name": "value", "ty": {"Named": "u32"}, "docs": "v"});
    register_dynamic_component(
        &mut reg,
        &desc(
            "Hp",
            json!({"Struct": {"fields": [field]}}),
            Some(json!({"value": 10})),
        ),
    )
    .unwrap();
    register_dynamic_component(&mut reg, &desc("Wings", json!("Unit"), Some(Value::Null))).unwrap();
    let n = json!({"name": "n", "ty": {"Named": "u32"}, "docs": "n"});
    register_dynamic_component(
        &mut reg,
        &desc("NoDefault", json!({"Struct": {"fields": [n]}}), None),
    )
    .unwrap();
    reg
}

fn flyer() -> DynamicTrait {
    DynamicTrait {
        name: "s.Flyer".into(),
        docs: "Flies.".into(),
        components: vec!["Hp".into(), "Wings".into()],
        requires: vec!["s.fly".into()],
    }
}

#[test]
fn dynamic_trait_adds_default_components_and_requires_processors() {
    let mut reg = registry();
    register_dynamic_trait(&mut reg, &flyer()).unwrap();
    assert!(trait_names(&reg).any(|t| t == "s.Flyer"));
    let mut t = Template::new("bird", "", [("Hp", json!({"value": 3}))]);
    t.traits.insert("s.Flyer".into(), json!({}));
    let compiled = t.compile(&reg).unwrap();
    assert_eq!(compiled.component_names(), ["Hp", "Wings"]);
    assert_eq!(compiled.requires(), ["s.fly"]);
    assert_eq!(
        compiled.components_json()["Hp"],
        json!({"value": 3}),
        "values override"
    );
    let mut plain = Template::new("plain", "", []);
    plain.traits.insert("s.Flyer".into(), json!({}));
    assert_eq!(
        plain.compile(&reg).unwrap().components_json()["Hp"],
        json!({"value": 10})
    );
}

#[test]
fn dynamic_trait_needs_registered_components_with_defaults() {
    let mut reg = registry();
    let mut unknown = flyer();
    unknown.components.push("Nope".into());
    let err = register_dynamic_trait(&mut reg, &unknown).unwrap_err();
    assert!(
        matches!(err, TemplateError::UnknownComponent { .. }),
        "{err}"
    );
    let mut no_default = flyer();
    no_default.components = vec!["NoDefault".into()];
    let err = register_dynamic_trait(&mut reg, &no_default)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("NoDefault") && err.contains("default"),
        "{err}"
    );
}

#[test]
fn reregistering_a_dynamic_trait_replaces_it() {
    let mut reg = registry();
    register_dynamic_trait(&mut reg, &flyer()).unwrap();
    let mut smaller = flyer();
    smaller.components = vec!["Wings".into()];
    register_dynamic_trait(&mut reg, &smaller).unwrap();
    let mut t = Template::new("bird", "", []);
    t.traits.insert("s.Flyer".into(), json!({}));
    assert_eq!(t.compile(&reg).unwrap().component_names(), ["Wings"]);
}

#[test]
fn compiled_template_records_a_deferred_spawn() {
    let mut reg = registry();
    register_dynamic_trait(&mut reg, &flyer()).unwrap();
    let mut t = Template::new("bird", "", []);
    t.traits.insert("s.Flyer".into(), json!({}));
    let compiled = t.compile(&reg).unwrap();
    let mut world = World::new();
    let mut cmds = CommandBuffer::new();
    let _pending = compiled.record(&mut cmds);
    assert_eq!(world.len(), 0, "nothing happens until the buffer applies");
    let mut ids = DeterministicIdGen::new(1);
    let report = cmds.apply(&mut world, &mut ids);
    assert_eq!(report.spawned.len(), 1);
    let id = report.spawned[0];
    assert_eq!(world.get_json(id, "Hp").unwrap(), json!({"value": 10}));
    assert!(world.get_json(id, "Wings").is_ok());
}
