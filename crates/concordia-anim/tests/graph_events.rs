//! Clip events inside graphs (2.4.4): a clip node fires when it advances with a non-zero share of
//! the pose (Godot's `AnimationMixer`), shares multiply down the tree, filtered blends keep their
//! first input at full share, and a tick's events are in crossing order. dt = 1/8 s throughout.

mod common;

use std::collections::BTreeMap;

use common::joint;
use concordia_anim::{AnimEvent, AnimGraphPlayer, BoundClip, BoundGraph, Pose, Skeleton};
use concordia_assets::{
    AdvanceMode, AnimGraph, AnimNode, AssetRef, Channel, ChannelPath, ClipData, ClipEvent,
    FadeCurve, Interpolation, Parameter, StateTransition, SwitchMode,
};
use concordia_core::math::Vec3;

const DT: f32 = 0.125;
const EPS: f32 = 1e-5;
const FILE: &str = "018bcfe5-6800-7000-a424-6c55076b137f";

fn skeleton() -> Skeleton {
    Skeleton::new(vec![
        joint("hip", None, Vec3::ZERO),
        joint("arm", Some(0), Vec3::Y),
    ])
    .unwrap()
}

fn clip_ref(name: &str) -> AssetRef {
    format!("asset:{FILE}#clip/{name}").parse().unwrap()
}

/// A 1 s clip holding `hip` still, with `events` (time, name).
fn marked(name: &str, events: &[(f32, &str)]) -> ClipData {
    ClipData {
        name: name.to_owned(),
        duration: 1.0,
        frame_rate: None,
        channels: vec![Channel {
            target: FILE.parse().unwrap(),
            target_name: "hip".into(),
            path: ChannelPath::Translation,
            interpolation: Interpolation::Linear,
            times: vec![0.0, 1.0],
            values: vec![0.0; 6],
        }],
        events: events
            .iter()
            .map(|(time, name)| ClipEvent {
                time: *time,
                name: (*name).to_owned(),
            })
            .collect(),
    }
}

fn clips() -> BTreeMap<AssetRef, ClipData> {
    [
        marked("walk", &[(0.125, "step")]),
        marked("run", &[(0.125, "stomp")]),
        marked("late", &[(0.2, "late")]),
        marked("early", &[(0.15, "early")]),
        marked("swing", &[(0.0, "swing")]),
        marked("idle", &[(0.0, "settle"), (0.125, "breathe")]),
        marked("jump", &[(0.0, "jump"), (0.125, "jump")]),
    ]
    .into_iter()
    .map(|c| (clip_ref(&c.name), c))
    .collect()
}

fn clip_node(clip: &str) -> AnimNode {
    AnimNode::Clip {
        clip: clip_ref(clip),
        looping: true,
    }
}

fn graph(parameters: &[(&str, Parameter)], nodes: Vec<(&str, AnimNode)>, root: &str) -> AnimGraph {
    AnimGraph {
        parameters: parameters
            .iter()
            .map(|(n, p)| ((*n).to_owned(), *p))
            .collect(),
        nodes: nodes.into_iter().map(|(n, d)| (n.to_owned(), d)).collect(),
        root: root.to_owned(),
    }
}

/// Plays a graph tick by tick, collecting its events.
struct Run {
    graph: BoundGraph,
    player: AnimGraphPlayer,
    state: concordia_anim::AnimGraphState,
    skeleton: Skeleton,
    pose: Pose,
}

impl Run {
    fn new(graph: &AnimGraph) -> Self {
        let skeleton = skeleton();
        let clips = clips();
        let bound = BoundGraph::bind(graph, &skeleton, &mut |c| {
            BoundClip::bind(&clips[c], &skeleton)
        })
        .unwrap();
        Self {
            state: bound.initial_state(),
            pose: skeleton.rest_pose(),
            player: AnimGraphPlayer::new(format!("asset:{FILE}").parse().unwrap()),
            graph: bound,
            skeleton,
        }
    }

    /// One tick: (event name, node, weight) in firing order.
    fn tick(&mut self) -> Vec<(String, String, f32)> {
        self.events()
            .into_iter()
            .map(|e| (e.name, e.node.unwrap_or_default(), e.weight))
            .collect()
    }

    fn events(&mut self) -> Vec<AnimEvent> {
        self.graph
            .evaluate(
                DT,
                &mut self.player,
                &mut self.state,
                &self.skeleton,
                &mut self.pose,
            )
            .unwrap()
    }
}

fn assert_events(got: &[(String, String, f32)], want: &[(&str, &str, f32)], what: &str) {
    let names = |v: &[(String, String, f32)]| {
        v.iter()
            .map(|(e, n, _)| format!("{e}@{n}"))
            .collect::<Vec<_>>()
    };
    let wanted: Vec<String> = want.iter().map(|(e, n, _)| format!("{e}@{n}")).collect();
    assert_eq!(names(got), wanted, "{what}");
    for ((_, _, a), (_, _, b)) in got.iter().zip(want) {
        assert!((a - b).abs() < EPS, "{what}: weight {a} vs {b} in {got:?}");
    }
}

#[test]
fn events_from_clips_inside_graphs_follow_the_refined_rule() {
    // Blend2 at 0.25: both clips cross 0.125 on tick 2; shares 0.75 and 0.25; ties by node name.
    let blend = |filter: Vec<String>| {
        graph(
            &[("amount", Parameter::Float(0.25))],
            vec![
                ("walk", clip_node("walk")),
                ("run", clip_node("run")),
                (
                    "gait",
                    AnimNode::Blend2 {
                        a: "walk".into(),
                        b: "run".into(),
                        amount: "amount".into(),
                        filter,
                    },
                ),
            ],
            "gait",
        )
    };
    let mut run = Run::new(&blend(vec![]));
    assert_events(&run.tick(), &[], "tick 1 covers [0, 0.125)");
    assert_events(
        &run.tick(),
        &[("stomp", "run", 0.25), ("step", "walk", 0.75)],
        "both blended clips fire, by node name at a tie",
    );
    // At amount 0 the second input still advances but has no share: it doesn't fire.
    let mut run = Run::new(&blend(vec![]));
    run.player.floats.insert("amount".into(), 0.0);
    run.tick();
    assert_events(
        &run.tick(),
        &[("step", "walk", 1.0)],
        "weight-0 inputs don't fire",
    );
    // A filtered blend keeps its first input at full share.
    let mut run = Run::new(&blend(vec!["arm".into()]));
    run.tick();
    assert_events(
        &run.tick(),
        &[("stomp", "run", 0.25), ("step", "walk", 1.0)],
        "filtered: the base keeps full share",
    );

    // Crossing order across clips: early (0.15) is crossed before late (0.2) in [0.125, 0.25),
    // though its node sorts second by name. BlendN 1:3 shares.
    let order = graph(
        &[("w1", Parameter::Float(1.0)), ("w3", Parameter::Float(3.0))],
        vec![
            ("a_late", clip_node("late")),
            ("b_early", clip_node("early")),
            (
                "mix",
                AnimNode::BlendN {
                    inputs: vec!["a_late".into(), "b_early".into()],
                    weights: vec!["w1".into(), "w3".into()],
                },
            ),
        ],
        "mix",
    );
    let mut run = Run::new(&order);
    run.tick();
    assert_events(
        &run.tick(),
        &[("early", "b_early", 0.75), ("late", "a_late", 0.25)],
        "chronological within the tick, though a_late sorts first by name",
    );

    // One-shot, linear fade-in 0.25: on the firing tick the shot has run 0.125 s, so its share
    // is 0.5 and the base's the rest; the shot's event at 0 fires as it starts.
    let shot = graph(
        &[("go", Parameter::Trigger)],
        vec![
            ("walk", clip_node("walk")),
            ("swing", clip_node("swing")),
            (
                "attack",
                AnimNode::OneShot {
                    base: "walk".into(),
                    shot: "swing".into(),
                    request: "go".into(),
                    fade_in: 0.25,
                    fade_out: 0.0,
                    curve: FadeCurve::Linear,
                    filter: vec![],
                },
            ),
        ],
        "attack",
    );
    let mut run = Run::new(&shot);
    run.tick();
    run.player.triggers.push("go".into());
    assert_events(
        &run.tick(),
        &[("swing", "swing", 0.5), ("step", "walk", 0.5)],
        "one-shot shares on its first tick (swing at the step's start)",
    );

    // A state machine fading walk → idle over 0.5 s: on the switch tick the destination has
    // 0.25 and the fading source 0.75; the inactive state never fires.
    let fade = graph(
        &[("rest", Parameter::Bool(false))],
        vec![
            ("walk", clip_node("walk")),
            ("idle", clip_node("idle")),
            ("jump", clip_node("jump")),
            (
                "moves",
                AnimNode::StateMachine {
                    states: [("walk", "walk"), ("idle", "idle"), ("jump", "jump")]
                        .into_iter()
                        .map(|(s, n)| (s.to_owned(), n.to_owned()))
                        .collect(),
                    start: "walk".into(),
                    transitions: vec![
                        StateTransition {
                            from: "walk".into(),
                            to: "idle".into(),
                            switch: SwitchMode::Immediate,
                            advance: AdvanceMode::Auto,
                            condition: Some("rest".into()),
                            xfade: 0.5,
                            curve: FadeCurve::Linear,
                            reset: true,
                            priority: 1,
                        },
                        StateTransition {
                            from: "idle".into(),
                            to: "jump".into(),
                            switch: SwitchMode::Immediate,
                            advance: AdvanceMode::Enabled,
                            condition: None,
                            xfade: 0.0,
                            curve: FadeCurve::Linear,
                            reset: true,
                            priority: 1,
                        },
                    ],
                },
            ),
        ],
        "moves",
    );
    let mut run = Run::new(&fade);
    run.tick();
    run.player.bools.insert("rest".into(), true);
    // Walk crosses 0.125 as idle starts at 0: settle (idle at 0) then step (walk, fraction 0).
    assert_events(
        &run.tick(),
        &[("settle", "idle", 0.25), ("step", "walk", 0.75)],
        "a cross-fade splits shares; ties by node name",
    );
    // Half-way through the fade, idle crosses 0.125 at a share of 0.5.
    assert_events(&run.tick(), &[("breathe", "idle", 0.5)], "still fading");
    for _ in 0..16 {
        assert!(
            run.tick().iter().all(|(e, _, _)| e != "jump"),
            "an inactive state never fires"
        );
    }

    // Every event carries its clip, node and clip time.
    let mut run = Run::new(&blend(vec![]));
    run.tick();
    let events = run.events();
    assert_eq!(
        events[0],
        AnimEvent {
            name: "stomp".into(),
            clip: clip_ref("run"),
            node: Some("run".into()),
            time: 0.125,
            weight: 0.25,
        }
    );
}

#[test]
fn time_scaled_graph_nodes_fire_backward() {
    let backward = graph(
        &[("scale", Parameter::Float(-1.0))],
        vec![
            ("idle", clip_node("idle")),
            (
                "back",
                AnimNode::TimeScale {
                    input: "idle".into(),
                    scale: "scale".into(),
                },
            ),
        ],
        "back",
    );
    let mut run = Run::new(&backward);
    // From 0 backward: (−0.125, 0] covers settle at 0 (lap 0); breathe at 0.125 is not crossed.
    assert_events(
        &run.tick(),
        &[("settle", "idle", 1.0)],
        "backward from the start",
    );
    // From 0.875: (0.75, 0.875] crosses nothing; ... down to (0.125, 0.25]: nothing; (0, 0.125]: breathe.
    let mut fired = Vec::new();
    for _ in 0..7 {
        fired.extend(run.tick());
    }
    assert_events(&fired, &[("breathe", "idle", 1.0)], "one lap backward");
}
