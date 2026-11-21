//! `.animgraph.ron` (M2 2.4.3): canonical text, validation naming the node, and import through the
//! asset database.

mod common;

use std::path::Path;

use common::Project;
use concordia_assets::{
    AdvanceMode, AnimGraph, AnimGraphError, AnimNode, AssetKind, AssetRef, FadeCurve, Parameter,
    StateTransition, SwitchMode, TransitionInput, read_animgraph, write_animgraph,
};

const FILE: &str = "018bcfe5-6800-7000-a424-6c55076b137f";

fn clip(name: &str) -> AnimNode {
    AnimNode::Clip {
        clip: format!("asset:{FILE}#clip/{name}").parse().unwrap(),
        looping: true,
    }
}

/// A graph using every node kind and parameter type.
fn character() -> AnimGraph {
    let mut g = AnimGraph {
        root: "moves".into(),
        ..AnimGraph::default()
    };
    for (name, p) in [
        ("speed", Parameter::Float(0.0)),
        ("rate", Parameter::Float(1.0)),
        ("w_left", Parameter::Float(1.0)),
        ("w_right", Parameter::Float(0.0)),
        ("stance", Parameter::Int(0)),
        ("grounded", Parameter::Bool(true)),
        ("attack", Parameter::Trigger),
    ] {
        g.parameters.insert(name.into(), p);
    }
    for (name, node) in [
        ("idle", clip("Idle")),
        ("walk", clip("Walk")),
        ("strafe_l", clip("StrafeL")),
        ("strafe_r", clip("StrafeR")),
        ("crouch", clip("Crouch")),
        (
            "swing",
            AnimNode::Clip {
                clip: format!("asset:{FILE}#clip/Swing").parse().unwrap(),
                looping: false,
            },
        ),
        ("fall", clip("Fall")),
        (
            "gait",
            AnimNode::Blend2 {
                a: "idle".into(),
                b: "walk".into(),
                amount: "speed".into(),
                filter: vec![],
            },
        ),
        (
            "strafe",
            AnimNode::BlendN {
                inputs: vec!["strafe_l".into(), "strafe_r".into()],
                weights: vec!["w_left".into(), "w_right".into()],
            },
        ),
        (
            "stances",
            AnimNode::Transition {
                inputs: vec![
                    TransitionInput {
                        node: "gait".into(),
                        reset: false,
                    },
                    TransitionInput {
                        node: "strafe".into(),
                        reset: true,
                    },
                    TransitionInput {
                        node: "crouch".into(),
                        reset: true,
                    },
                ],
                select: "stance".into(),
                xfade: 0.2,
                curve: FadeCurve::Smooth,
            },
        ),
        (
            "paced",
            AnimNode::TimeScale {
                input: "stances".into(),
                scale: "rate".into(),
            },
        ),
        (
            "ground",
            AnimNode::OneShot {
                base: "paced".into(),
                shot: "swing".into(),
                request: "attack".into(),
                fade_in: 0.1,
                fade_out: 0.25,
                curve: FadeCurve::Linear,
                filter: vec!["Spine".into(), "Arm.R".into()],
            },
        ),
        (
            "moves",
            AnimNode::StateMachine {
                states: [("ground", "ground"), ("air", "fall")]
                    .into_iter()
                    .map(|(s, n)| (s.to_owned(), n.to_owned()))
                    .collect(),
                start: "ground".into(),
                transitions: vec![
                    StateTransition {
                        from: "ground".into(),
                        to: "air".into(),
                        switch: SwitchMode::Immediate,
                        advance: AdvanceMode::Auto,
                        condition: None,
                        xfade: 0.15,
                        curve: FadeCurve::Linear,
                        reset: true,
                        priority: 1,
                    },
                    StateTransition {
                        from: "air".into(),
                        to: "ground".into(),
                        switch: SwitchMode::AtEnd,
                        advance: AdvanceMode::Auto,
                        condition: Some("grounded".into()),
                        xfade: 0.1,
                        curve: FadeCurve::Smooth,
                        reset: false,
                        priority: 2,
                    },
                ],
            },
        ),
    ] {
        g.nodes.insert(name.into(), node);
    }
    g
}

#[test]
fn animgraph_roundtrips_canonical() {
    let graph = character();
    graph.validate().unwrap();
    let text = write_animgraph(&graph).unwrap();
    insta::assert_snapshot!("character_canonical", text);
    assert_eq!(read_animgraph(&text).unwrap(), graph, "text → graph → text");
    assert_eq!(
        write_animgraph(&read_animgraph(&text).unwrap()).unwrap(),
        text,
        "canonical text is a fixed point"
    );
}

#[test]
fn omitted_fields_take_their_defaults() {
    let text = format!(
        r#"(
            parameters: {{"go": Bool(false)}},
            nodes: {{
                "a": Clip(clip: "asset:{FILE}#clip/A"),
                "b": Clip(clip: "asset:{FILE}#clip/B", looping: false),
                "m": StateMachine(
                    states: {{"one": "a", "two": "b"}},
                    start: "one",
                    transitions: [(from: "one", to: "two", condition: Some("go"))],
                ),
            }},
            root: "m",
        )"#
    );
    let graph = read_animgraph(&text).unwrap();
    assert_eq!(
        graph.node("a"),
        Some(&AnimNode::Clip {
            clip: format!("asset:{FILE}#clip/A").parse().unwrap(),
            looping: true,
        })
    );
    let Some(AnimNode::StateMachine { transitions, .. }) = graph.node("m") else {
        panic!("a state machine");
    };
    assert_eq!(
        transitions[0],
        StateTransition {
            from: "one".into(),
            to: "two".into(),
            switch: SwitchMode::Immediate,
            advance: AdvanceMode::Enabled,
            condition: Some("go".into()),
            xfade: 0.0,
            curve: FadeCurve::Linear,
            reset: true,
            priority: 1,
        },
        "Godot's transition defaults"
    );
    let clips: Vec<String> = graph.clips().iter().map(|r| r.to_string()).collect();
    assert_eq!(
        clips,
        [
            format!("asset:{FILE}#clip/A"),
            format!("asset:{FILE}#clip/B")
        ]
    );
}

/// `character()` changed by `edit`, validated.
fn invalid(edit: impl FnOnce(&mut AnimGraph)) -> AnimGraphError {
    let mut g = character();
    edit(&mut g);
    g.validate().expect_err("the edit makes the graph invalid")
}

fn node_mut<'a>(g: &'a mut AnimGraph, name: &str) -> &'a mut AnimNode {
    g.nodes.get_mut(name).unwrap()
}

fn transitions(g: &mut AnimGraph) -> &mut Vec<StateTransition> {
    match node_mut(g, "moves") {
        AnimNode::StateMachine { transitions, .. } => transitions,
        _ => unreachable!(),
    }
}

#[track_caller]
fn names(e: &AnimGraphError, node: &str, words: &[&str]) {
    assert_eq!(e.node(), Some(node), "{e}");
    let text = e.to_string();
    for w in words {
        assert!(text.contains(w), "{text:?} should mention {w:?}");
    }
}

#[test]
fn graph_validation_names_the_node() {
    let e = invalid(|g| g.root = "nope".into());
    assert!(matches!(e, AnimGraphError::MissingRoot { .. }), "{e}");

    let e = invalid(|g| {
        let n = g.nodes.remove("fall").unwrap();
        g.nodes.insert("fa ll".into(), n);
    });
    names(&e, "fa ll", &["letters"]);

    let e = invalid(|g| {
        if let AnimNode::Blend2 { b, .. } = node_mut(g, "gait") {
            *b = "ghost".into();
        }
    });
    names(&e, "gait", &["ghost"]);

    let e = invalid(|g| {
        if let AnimNode::Blend2 { amount, .. } = node_mut(g, "gait") {
            *amount = "nope".into();
        }
    });
    names(&e, "gait", &["nope", "unknown parameter"]);

    let e = invalid(|g| {
        if let AnimNode::Blend2 { amount, .. } = node_mut(g, "gait") {
            *amount = "grounded".into();
        }
    });
    names(&e, "gait", &["grounded", "Float"]);

    let e = invalid(|g| {
        if let AnimNode::OneShot { request, .. } = node_mut(g, "ground") {
            *request = "speed".into();
        }
    });
    names(&e, "ground", &["speed", "Trigger"]);

    let e = invalid(|g| {
        if let AnimNode::Transition { select, .. } = node_mut(g, "stances") {
            *select = "speed".into();
        }
    });
    names(&e, "stances", &["speed", "Int"]);

    let e = invalid(|g| transitions(g)[1].condition = Some("speed".into()));
    names(&e, "moves", &["speed", "Bool or Trigger"]);

    let e = invalid(|g| {
        if let AnimNode::TimeScale { input, .. } = node_mut(g, "paced") {
            *input = "ground".into();
        }
    });
    let AnimGraphError::Cycle { path } = &e else {
        panic!("a cycle: {e}");
    };
    assert_eq!(path.first(), path.last(), "{e}");
    assert!(path.contains(&"paced".to_owned()), "{e}");

    let e = invalid(|g| {
        g.nodes.insert("orphan".into(), clip("Idle"));
    });
    names(&e, "orphan", &["not reachable"]);

    let e = invalid(|g| {
        if let AnimNode::Transition { inputs, .. } = node_mut(g, "stances") {
            inputs[2].node = "idle".into();
        }
        g.nodes.remove("crouch");
    });
    names(&e, "idle", &["gait", "stances"]);
}

#[test]
fn state_machine_validation_names_the_machine() {
    let e = invalid(|g| {
        if let AnimNode::StateMachine { start, .. } = node_mut(g, "moves") {
            *start = "nowhere".into();
        }
    });
    names(&e, "moves", &["nowhere", "start"]);

    let e = invalid(|g| transitions(g)[0].to = "nowhere".into());
    names(&e, "moves", &["nowhere"]);

    let e = invalid(|g| transitions(g)[0].to = "ground".into());
    names(&e, "moves", &["itself"]);

    let e = invalid(|g| transitions(g)[0].advance = AdvanceMode::Disabled);
    names(&e, "moves", &["air", "not reachable"]);

    let e = invalid(|g| {
        if let AnimNode::StateMachine { states, .. } = node_mut(g, "moves") {
            states.insert("air".into(), "ghost".into());
        }
        g.nodes.remove("fall");
    });
    names(&e, "moves", &["ghost"]);

    let e = invalid(|g| transitions(g)[0].xfade = -1.0);
    names(&e, "moves", &["xfade"]);
}

#[test]
fn bad_weights_and_values_are_errors() {
    let e = invalid(|g| {
        g.parameters.insert("speed".into(), Parameter::Float(1.5));
    });
    names(&e, "gait", &["speed", "0..=1"]);

    let e = invalid(|g| {
        g.parameters
            .insert("w_right".into(), Parameter::Float(-1.0));
    });
    names(&e, "strafe", &["w_right", "negative"]);

    let e = invalid(|g| {
        if let AnimNode::BlendN { weights, .. } = node_mut(g, "strafe") {
            weights.pop();
        }
    });
    names(&e, "strafe", &["2 inputs", "1 weights"]);

    let e = invalid(|g| {
        g.parameters.insert("stance".into(), Parameter::Int(3));
    });
    names(&e, "stances", &["stance", "3 inputs"]);

    let e = invalid(|g| {
        if let AnimNode::OneShot { fade_out, .. } = node_mut(g, "ground") {
            *fade_out = f32::NAN;
        }
    });
    names(&e, "ground", &["fade_out"]);

    let e = invalid(|g| {
        g.parameters
            .insert("rate".into(), Parameter::Float(f32::INFINITY));
    });
    assert!(
        matches!(&e, AnimGraphError::Parameter { parameter, .. } if parameter == "rate"),
        "{e}"
    );

    let e = invalid(|g| {
        g.parameters.insert("bad name".into(), Parameter::Trigger);
    });
    assert!(e.to_string().contains("bad name"), "{e}");
}

#[test]
fn animgraph_imports_through_the_asset_db() {
    let mut p = Project::new();
    std::fs::create_dir_all(p.path("anim")).unwrap();
    let text = write_animgraph(&character()).unwrap();
    std::fs::write(p.path("anim/hero.animgraph.ron"), &text).unwrap();
    let path = Path::new("anim/hero.animgraph.ron");
    assert_eq!(p.db.importer_name(path), Some("animgraph"));
    let report = p.db.import(path).unwrap();
    let reference: AssetRef = format!("asset:{}", report.id).parse().unwrap();
    let graph: AnimGraph = p.db.load(&reference, AssetKind::AnimGraph).unwrap();
    assert_eq!(graph, character());
    let again = p.db.import(path).unwrap();
    assert_eq!(again.id, report.id, "the sidecar keeps the id");
    assert!(again.cached);

    let broken = text.replace("start: \"ground\"", "start: \"ghost\"");
    assert_ne!(broken, text, "the edit applies");
    std::fs::write(p.path("anim/broken.animgraph.ron"), broken).unwrap();
    let e =
        p.db.import(Path::new("anim/broken.animgraph.ron"))
            .unwrap_err()
            .to_string();
    assert!(e.contains("moves") && e.contains("ghost"), "{e}");
}

#[test]
fn animgraph_is_registered_for_schema_get() {
    let mut registry = concordia_reflect::TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_assets::register_types(&mut registry).unwrap();
    assert!(registry.get_by_name("AnimGraph").is_some());
}
