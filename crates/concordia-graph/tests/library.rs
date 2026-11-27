//! The node library (M4 4.9, D6): built from what the engine exposes, typed, stable.

use concordia_graph::{NodeLibrary, PinType};
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
    reg.register::<Damaged>().unwrap();
    // Components are what the caller says (terra's registrations in the engine).
    NodeLibrary::build(&reg, |r| ["Health", "Damaged"].contains(&r.info().name))
}

#[test]
fn node_library_snapshot_is_stable() {
    let core = NodeLibrary::core();
    insta::assert_json_snapshot!("core_library", core.to_json());
}

#[test]
fn reflected_component_get_set_nodes_have_correct_types() {
    let lib = library();
    let get = lib.get("get.Health").expect("get.Health");
    assert!(get.pure && get.exec_in.is_empty());
    assert_eq!(get.input("entity").unwrap().ty, PinType::EntityId);
    assert_eq!(
        get.output("value").unwrap().ty,
        PinType::Named("Health".into())
    );
    let set = lib.get("set.Health").expect("set.Health");
    assert!(!set.pure && set.exec_in == ["in"] && set.exec_out == ["out"]);
    assert_eq!(
        set.input("value").unwrap().ty,
        PinType::Named("Health".into())
    );
    let field = lib.get("get.Health.max").expect("a field getter");
    assert_eq!(field.output("value").unwrap().ty, PinType::F32);
    // A signal is raised and received, not got or set.
    assert!(lib.get("get.Damaged").is_none() && lib.get("set.Damaged").is_none());
    let raise = lib.get("raise.Damaged").expect("raise.Damaged");
    assert!(!raise.pure);
    assert_eq!(raise.input("target").unwrap().ty, PinType::EntityId);
    let on = lib.get("on.Damaged").expect("on.Damaged");
    assert!(on.event && on.exec_out == ["out"]);
    assert_eq!(
        on.output("signal").unwrap().ty,
        PinType::Named("Damaged".into())
    );
}

#[test]
fn graph_node_functions_join_the_library() {
    let info = concordia_graph::fns::length_graph_node();
    assert_eq!(info.name, "length");
    assert!(info.pure);
    assert_eq!(info.inputs[0].0, "v");
    assert_eq!(info.inputs[0].1.name(), "Vec3");
    assert_eq!(info.output.as_ref().unwrap().name(), "f32");
    let lib = NodeLibrary::core();
    let node = lib.get("vector.length").expect("vector.length");
    assert_eq!(node.input("v").unwrap().ty, PinType::Vec3);
    assert_eq!(node.output("out").unwrap().ty, PinType::F32);
}

#[test]
fn pin_types_parse_from_their_names() {
    for (text, ty) in [
        ("f32", PinType::F32),
        ("EntityId", PinType::EntityId),
        ("List<EntityId>", PinType::List(Box::new(PinType::EntityId))),
        ("Option<Vec3>", PinType::Option(Box::new(PinType::Vec3))),
        ("Health", PinType::Named("Health".into())),
    ] {
        assert_eq!(PinType::parse(text), Ok(ty.clone()), "{text}");
        assert_eq!(ty.to_string(), text);
    }
    assert!(PinType::parse("List<").is_err());
    assert!(PinType::parse("").is_err());
}

#[test]
fn fields_can_be_set_and_signals_carry_their_fields() {
    let lib = library();
    let set = lib.get("set.Health.max").expect("a field setter");
    assert!(!set.pure && set.exec_in == ["in"] && set.exec_out == ["out"]);
    assert_eq!(set.input("value").unwrap().ty, PinType::F32);
    // A signal is raised from its fields and received with them.
    let raise = lib.get("raise.Damaged").unwrap();
    let names: Vec<&str> = raise.inputs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["target", "amount"]);
    assert!(raise.input("amount").unwrap().required);
    let on = lib.get("on.Damaged").unwrap();
    assert_eq!(on.output("amount").unwrap().ty, PinType::F32);
}
