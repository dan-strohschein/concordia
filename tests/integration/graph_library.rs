//! The graph node library over the real engine (M4 4.9, D6): every reflected component gets
//! `get.`/`set.` nodes and every signal `raise.`/`on.` nodes, including a game's script types.

/// The engine's signals (`#[reflect(signal)]`), all of which must be marked.
const ENGINE_SIGNALS: [&str; 17] = [
    "ContactBegan",
    "ContactEnded",
    "AnimEvent",
    "NavArrived",
    "NavPathFailed",
    "PlayCue",
    "StopCue",
    "SetBusGain",
    "UiPressed",
    "UiToggled",
    "UiValueChanged",
    "UiSelected",
    "UiFocusChanged",
    "UiTextChanged",
    "UiTextSubmitted",
    "VehicleArrived",
    "VehicleRouteFailed",
];

#[test]
fn every_reflected_component_and_signal_has_nodes() {
    let root = concordia_tests::repo_root();
    let project = concordia_app::Project::load(&root.join("examples/m2-lamplight")).unwrap();
    let engine = concordia_api::Engine::new(project.engine_config().unwrap()).unwrap();
    let registry = engine.registry();
    let is_component = |r: &concordia_reflect::TypeRegistration| {
        r.data::<concordia_terra::ReflectComponent>().is_some()
    };
    let library = concordia_graph::NodeLibrary::build(registry, is_component);
    let mut components = 0;
    let mut signals = Vec::new();
    for name in registry.names() {
        let r = registry.get_by_name(name).unwrap();
        if !is_component(r) {
            continue;
        }
        match concordia_reflect::is_signal(r) {
            true => {
                signals.push(name);
                assert!(
                    library.get(&format!("raise.{name}")).is_some(),
                    "raise.{name}"
                );
                assert!(library.get(&format!("on.{name}")).is_some(), "on.{name}");
            }
            false => {
                components += 1;
                assert!(library.get(&format!("get.{name}")).is_some(), "get.{name}");
                assert!(library.get(&format!("set.{name}")).is_some(), "set.{name}");
            }
        }
    }
    println!(
        "{components} components, {} signals: {signals:?}",
        signals.len()
    );
    for s in ENGINE_SIGNALS {
        if registry.get_by_name(s).is_some() {
            assert!(signals.contains(&s), "{s} is registered but not a signal");
        }
    }
    // Lamplight's script module declares signals of its own; they count too.
    assert!(
        signals.iter().any(|s| s.starts_with("lamplight.")),
        "a script module's signals: {signals:?}"
    );
    assert!(components > 50, "{components}");
    // Signal and component fields become pins: no name may collide with a fixed one.
    for node in library.nodes() {
        for pins in [&node.inputs, &node.outputs] {
            let mut names: Vec<&str> = pins.iter().map(|p| p.name.as_str()).collect();
            names.sort_unstable();
            let before = names.len();
            names.dedup();
            assert_eq!(names.len(), before, "{} has duplicate pin names", node.op);
        }
    }
}
