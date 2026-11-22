//! Graph evaluation (2.4.3): blends, one-shots, time scale, transitions and state machines, on a
//! hand-built two-bone skeleton with constant and ramp clips so every expected pose is hand-computed.

mod common;

use std::collections::BTreeMap;

use common::{close, joint};
use concordia_anim::{
    AnimError, AnimGraphPlayer, AnimGraphState, BoundClip, BoundGraph, Pose, Skeleton,
};
use concordia_assets::{
    AdvanceMode, AnimGraph, AnimNode, AssetRef, Channel, ChannelPath, ClipData, FadeCurve,
    Interpolation, Parameter, StateTransition, SwitchMode, TransitionInput,
};
use concordia_core::math::{Quat, Vec3};

/// Ticks of 1/8 s (exact in binary, so clip ends fall on ticks).
const DT: f32 = 0.125;
const EPS: f32 = 1e-5;
const FILE: &str = "018bcfe5-6800-7000-a424-6c55076b137f";

/// `hip` at the origin, `arm` above it.
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

fn channel(bone: &str, path: ChannelPath, times: &[f32], values: &[f32]) -> Channel {
    Channel {
        target: FILE.parse().unwrap(),
        target_name: bone.to_owned(),
        path,
        interpolation: Interpolation::Linear,
        times: times.to_vec(),
        values: values.to_vec(),
    }
}

/// A clip of `duration` holding `hip` at x = `x` (and `arm` too, if `arm` is given).
fn hold(name: &str, duration: f32, x: f32, arm: Option<f32>) -> ClipData {
    let mut channels = vec![channel(
        "hip",
        ChannelPath::Translation,
        &[0.0, duration],
        &[x, 0.0, 0.0, x, 0.0, 0.0],
    )];
    if let Some(a) = arm {
        channels.push(channel(
            "arm",
            ChannelPath::Translation,
            &[0.0, duration],
            &[a, 0.0, 0.0, a, 0.0, 0.0],
        ));
    }
    ClipData {
        name: name.to_owned(),
        duration,
        frame_rate: None,
        events: Vec::new(),
        channels,
    }
}

/// A clip whose `hip` x is `offset` + its playback time (so the pose shows the clip's time).
fn ramp(name: &str, duration: f32, offset: f32) -> ClipData {
    ClipData {
        name: name.to_owned(),
        duration,
        frame_rate: None,
        events: Vec::new(),
        channels: vec![channel(
            "hip",
            ChannelPath::Translation,
            &[0.0, duration],
            &[offset, 0.0, 0.0, offset + duration, 0.0, 0.0],
        )],
    }
}

/// A clip holding `hip` at rotation `q` (and x = 0).
fn turn(name: &str, q: Quat) -> ClipData {
    let mut clip = hold(name, 1.0, 0.0, None);
    let v = q.to_array();
    clip.channels.push(channel(
        "hip",
        ChannelPath::Rotation,
        &[0.0, 1.0],
        &[v[0], v[1], v[2], v[3], v[0], v[1], v[2], v[3]],
    ));
    clip
}

/// Every fixture clip, by name.
fn clips() -> BTreeMap<AssetRef, ClipData> {
    let z90 = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    [
        hold("one", 1.0, 1.0, None),
        hold("three", 1.0, 3.0, None),
        hold("five", 1.0, 5.0, None),
        hold("ten", 1.0, 10.0, None),
        hold("shot", 1.0, 5.0, Some(5.0)),
        ramp("ramp", 2.0, 0.0),
        ramp("ramp10", 2.0, 10.0),
        ramp("ramp100", 2.0, 100.0),
        ramp("short", 0.5, 0.0),
        turn("still", Quat::IDENTITY),
        turn("turn", z90),
        turn("turn_neg", -z90),
    ]
    .into_iter()
    .map(|c| (clip_ref(&c.name), c))
    .collect()
}

fn clip_node(clip: &str, looping: bool) -> AnimNode {
    AnimNode::Clip {
        clip: clip_ref(clip),
        looping,
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

fn transition(from: &str, to: &str) -> StateTransition {
    StateTransition {
        from: from.to_owned(),
        to: to.to_owned(),
        switch: SwitchMode::Immediate,
        advance: AdvanceMode::Enabled,
        condition: None,
        xfade: 0.0,
        curve: FadeCurve::Linear,
        reset: true,
        priority: 1,
    }
}

fn auto(from: &str, to: &str, condition: Option<&str>) -> StateTransition {
    StateTransition {
        advance: AdvanceMode::Auto,
        condition: condition.map(str::to_owned),
        ..transition(from, to)
    }
}

fn machine(states: &[(&str, &str)], start: &str, transitions: Vec<StateTransition>) -> AnimNode {
    AnimNode::StateMachine {
        states: states
            .iter()
            .map(|(s, n)| ((*s).to_owned(), (*n).to_owned()))
            .collect(),
        start: start.to_owned(),
        transitions,
    }
}

fn bind(graph: &AnimGraph) -> Result<BoundGraph, AnimError> {
    let skeleton = skeleton();
    let all = clips();
    BoundGraph::bind(graph, &skeleton, &mut |r| {
        let data = all.get(r).ok_or_else(|| AnimError::ClipHeader {
            clip: r.to_string(),
            reason: "not found".to_owned(),
        })?;
        BoundClip::bind(data, &skeleton)
    })
}

/// A bound graph being played.
struct Run {
    graph: BoundGraph,
    skeleton: Skeleton,
    player: AnimGraphPlayer,
    state: AnimGraphState,
    pose: Pose,
}

impl Run {
    fn new(graph: &AnimGraph) -> Self {
        let bound = bind(graph).unwrap();
        let skeleton = skeleton();
        let state = bound.initial_state();
        let pose = skeleton.rest_pose();
        Self {
            graph: bound,
            skeleton,
            player: AnimGraphPlayer::new(format!("asset:{FILE}").parse().unwrap()),
            state,
            pose,
        }
    }

    fn tick(&mut self) -> &mut Self {
        self.graph
            .evaluate(
                DT,
                &mut self.player,
                &mut self.state,
                &self.skeleton,
                &mut self.pose,
            )
            .unwrap();
        self
    }

    fn ticks(&mut self, n: usize) -> &mut Self {
        for _ in 0..n {
            self.tick();
        }
        self
    }

    fn x(&self) -> f32 {
        self.pose.bones[0].translation.x
    }

    fn arm(&self) -> Vec3 {
        self.pose.bones[1].translation
    }

    fn hip_rotation(&self) -> Quat {
        self.pose.bones[0].rotation
    }

    fn float(&mut self, name: &str, v: f32) -> &mut Self {
        self.player.floats.insert(name.to_owned(), v);
        self
    }

    fn int(&mut self, name: &str, v: i32) -> &mut Self {
        self.player.ints.insert(name.to_owned(), v);
        self
    }

    fn flag(&mut self, name: &str, v: bool) -> &mut Self {
        self.player.bools.insert(name.to_owned(), v);
        self
    }

    fn fire(&mut self, name: &str) -> &mut Self {
        self.player.triggers.push(name.to_owned());
        self
    }

    fn travel(&mut self, machine: &str, to: &str) -> &mut Self {
        self.player.travel.insert(machine.to_owned(), to.to_owned());
        self
    }

    fn state_of(&self, machine: &str) -> Option<&str> {
        self.graph.active_state(&self.state, machine)
    }

    #[track_caller]
    fn expect_x(&self, x: f32) {
        assert!((self.x() - x).abs() < EPS, "hip x {} != {x}", self.x());
    }
}

/// The angle of a rotation about +Z, in degrees.
fn z_degrees(q: Quat) -> f32 {
    let (axis, angle) = q.to_axis_angle();
    angle.to_degrees() * axis.z.signum()
}

/// `Blend2` of clips `a` and `b` by the float parameter `amount` (default 0.5).
fn blend2(a: &str, b: &str) -> AnimGraph {
    graph(
        &[("amount", Parameter::Float(0.5))],
        vec![
            ("a", clip_node(a, true)),
            ("b", clip_node(b, true)),
            (
                "root",
                AnimNode::Blend2 {
                    a: "a".into(),
                    b: "b".into(),
                    amount: "amount".into(),
                    filter: vec![],
                },
            ),
        ],
        "root",
    )
}

#[test]
fn blend2_at_half_weight_is_normalized_average_pose() {
    let mut run = Run::new(&blend2("one", "three"));
    run.tick().expect_x(2.0);
    assert!(
        close(run.arm(), Vec3::Y, EPS),
        "unanimated bones stay at rest"
    );
    run.float("amount", 0.25).tick().expect_x(1.5);
    run.float("amount", 1.5).tick().expect_x(3.0);
    run.float("amount", -1.0).tick().expect_x(1.0);

    let mut run = Run::new(&blend2("still", "turn"));
    run.tick();
    let half = z_degrees(run.hip_rotation());
    assert!((half - 45.0).abs() < 1e-3, "half way: {half}°");
    run.float("amount", 0.25).tick();
    let quarter = z_degrees(run.hip_rotation());
    assert!(
        (quarter - 22.5).abs() < 1e-3,
        "slerp at a quarter is 22.5° (nlerp would be 21.6°): {quarter}°"
    );
    assert!((run.hip_rotation().length() - 1.0).abs() < EPS);
}

#[test]
fn blend_n_normalizes_weights_and_aligns_hemispheres() {
    let g = graph(
        &[("w1", Parameter::Float(1.0)), ("w2", Parameter::Float(3.0))],
        vec![
            ("a", clip_node("one", true)),
            ("b", clip_node("three", true)),
            (
                "root",
                AnimNode::BlendN {
                    inputs: vec!["a".into(), "b".into()],
                    weights: vec!["w1".into(), "w2".into()],
                },
            ),
        ],
        "root",
    );
    let mut run = Run::new(&g);
    run.tick().expect_x(0.25 * 1.0 + 0.75 * 3.0);
    run.float("w1", 0.0).float("w2", 0.0).tick().expect_x(1.0);
    run.float("w1", 2.0).float("w2", -5.0).tick().expect_x(1.0);

    let rotations = |b: &str| {
        graph(
            &[("w1", Parameter::Float(1.0)), ("w2", Parameter::Float(3.0))],
            vec![
                ("a", clip_node("still", true)),
                ("b", clip_node(b, true)),
                (
                    "root",
                    AnimNode::BlendN {
                        inputs: vec!["a".into(), "b".into()],
                        weights: vec!["w1".into(), "w2".into()],
                    },
                ),
            ],
            "root",
        )
    };
    // normalize(0.25 · (0, 0, 0, 1) + 0.75 · (0, 0, sin 45°, cos 45°)) = (0, 0, 0.53033, 0.78033) / 0.94348.
    let expected = Quat::from_xyzw(0.0, 0.0, 0.562_095, 0.827_070);
    for b in ["turn", "turn_neg"] {
        let mut run = Run::new(&rotations(b));
        run.tick();
        let q = run.hip_rotation();
        let same = q.dot(expected).abs();
        assert!(
            (same - 1.0).abs() < 1e-5,
            "{b}: weighted quaternion sum, flipped into the first input's hemisphere: {q}"
        );
    }
}

/// A one-shot of `shot` (hip and arm at 5, 1 s) over `one` (hip at 1), fading in 0.25 s and out 0.5 s.
fn one_shot(curve: FadeCurve, filter: &[&str]) -> AnimGraph {
    graph(
        &[("attack", Parameter::Trigger)],
        vec![
            ("base", clip_node("one", true)),
            ("swing", clip_node("shot", false)),
            (
                "root",
                AnimNode::OneShot {
                    base: "base".into(),
                    shot: "swing".into(),
                    request: "attack".into(),
                    fade_in: 0.25,
                    fade_out: 0.5,
                    curve,
                    filter: filter.iter().map(|b| (*b).to_owned()).collect(),
                },
            ),
        ],
        "root",
    )
}

#[test]
fn one_shot_overlays_then_fades_out_on_completion() {
    let mut run = Run::new(&one_shot(FadeCurve::Linear, &[]));
    run.ticks(2).expect_x(1.0);
    // Weight = min(elapsed / 0.25, remaining / 0.5): x = 1 + 4 · weight.
    run.fire("attack").tick().expect_x(3.0);
    assert!(
        run.player.triggers.is_empty(),
        "triggers clear after the evaluation"
    );
    for x in [5.0, 5.0, 5.0, 4.0, 3.0, 2.0] {
        run.tick().expect_x(x);
    }
    run.tick().expect_x(1.0);
    run.ticks(3).expect_x(1.0);

    // Firing again while active restarts the shot.
    run.fire("attack").ticks(3).expect_x(5.0);
    run.fire("attack").tick().expect_x(3.0);

    // Smoothstep: 3w² − 2w³ of the linear weight.
    let mut run = Run::new(&one_shot(FadeCurve::Smooth, &[]));
    run.fire("attack").tick().expect_x(3.0);
    run.ticks(4).expect_x(1.0 + 4.0 * 0.843_75);
    run.ticks(2).expect_x(1.0 + 4.0 * 0.156_25);

    // A bone filter: only the arm takes the shot; the hip keeps the base.
    let mut run = Run::new(&one_shot(FadeCurve::Linear, &["arm"]));
    run.fire("attack").tick();
    run.expect_x(1.0);
    assert!(
        close(run.arm(), Vec3::new(2.5, 0.5, 0.0), EPS),
        "half way from rest (0, 1, 0) to (5, 0, 0): {}",
        run.arm()
    );
    run.tick();
    run.expect_x(1.0);
    assert!(
        close(run.arm(), Vec3::new(5.0, 0.0, 0.0), EPS),
        "{}",
        run.arm()
    );
}

#[test]
fn time_scale_node_scales_playback_rate() {
    let g = graph(
        &[("speed", Parameter::Float(1.0))],
        vec![
            ("clip", clip_node("ramp", false)),
            (
                "root",
                AnimNode::TimeScale {
                    input: "clip".into(),
                    scale: "speed".into(),
                },
            ),
        ],
        "root",
    );
    let mut run = Run::new(&g);
    run.tick().expect_x(0.125);
    run.float("speed", 2.0).ticks(2).expect_x(0.625);
    run.float("speed", 0.5).ticks(2).expect_x(0.75);
    run.float("speed", 0.0).ticks(3).expect_x(0.75);
    run.float("speed", -1.0).tick().expect_x(0.625);
}

#[test]
fn transition_node_switches_input_with_xfade_and_reset() {
    let g = graph(
        &[("pick", Parameter::Int(0))],
        vec![
            ("ra", clip_node("ramp", true)),
            ("rb", clip_node("ramp10", true)),
            (
                "root",
                AnimNode::Transition {
                    inputs: vec![
                        TransitionInput {
                            node: "ra".into(),
                            reset: true,
                        },
                        TransitionInput {
                            node: "rb".into(),
                            reset: false,
                        },
                    ],
                    select: "pick".into(),
                    xfade: 0.25,
                    curve: FadeCurve::Linear,
                },
            ),
        ],
        "root",
    );
    let mut run = Run::new(&g);
    for x in [0.125, 0.25, 0.375, 0.5] {
        run.tick().expect_x(x);
    }
    // To rb (never played, so at 0): both advance, half way through the 0.25 s fade.
    run.int("pick", 1).tick().expect_x((0.625 + 10.125) / 2.0);
    run.tick().expect_x(10.25);
    // Back to ra, which resets.
    run.int("pick", 0).tick().expect_x((10.375 + 0.125) / 2.0);
    run.tick().expect_x(0.25);
    // To rb, which doesn't reset: it continues from 0.375.
    run.int("pick", 1).tick().expect_x((0.375 + 10.5) / 2.0);
    // An input that doesn't exist is ignored.
    run.int("pick", 7).tick().expect_x(10.625);
    run.int("pick", -1).tick().expect_x(10.75);
}

/// States `a`..`h`, `y` and `z` (hip at a distinct x each), joined so that several paths tie.
fn travel_graph() -> AnimGraph {
    let states = ["a", "b", "c", "d", "e", "f", "g", "h", "y", "z"];
    let mut nodes: Vec<(&str, AnimNode)> = Vec::new();
    let names: Vec<String> = states.iter().map(|s| format!("{s}_clip")).collect();
    let clip_for = [
        "one", "three", "five", "ten", "ramp", "ramp10", "ramp100", "short", "still", "turn",
    ];
    for (name, clip) in names.iter().zip(clip_for) {
        nodes.push((name.as_str(), clip_node(clip, true)));
    }
    let t = |from: &str, to: &str, priority: u32| StateTransition {
        priority,
        ..transition(from, to)
    };
    let transitions = vec![
        t("a", "b", 1),
        t("b", "c", 1),
        t("c", "d", 1),
        StateTransition {
            advance: AdvanceMode::Disabled,
            ..t("a", "c", 1)
        },
        t("a", "f", 1),
        t("f", "d", 1),
        t("a", "e", 1),
        t("e", "d", 1),
        t("a", "g", 0),
        t("g", "d", 1),
        t("f", "h", 1),
        t("e", "h", 1),
        t("d", "a", 1),
        t("h", "a", 1),
        t("a", "z", 1),
        t("z", "y", 1),
    ];
    let state_list: Vec<(&str, &str)> = states
        .iter()
        .zip(&names)
        .map(|(s, n)| (*s, n.as_str()))
        .collect();
    nodes.push(("moves", machine(&state_list, "a", transitions)));
    graph(&[], nodes, "moves")
}

#[test]
fn state_machine_travel_finds_shortest_transition_path() {
    let mut run = Run::new(&travel_graph());
    run.tick();
    assert_eq!(run.state_of("moves"), Some("a"));
    // a → d: a→b→c→d is 3 steps and a→c is disabled; a→f→d, a→e→d and a→g→d tie at 2,
    // and a→g has the lowest priority number.
    run.travel("moves", "d").tick();
    assert!(
        run.player.travel.is_empty(),
        "travel requests clear after the evaluation"
    );
    assert_eq!(run.state_of("moves"), Some("g"));
    run.tick();
    assert_eq!(run.state_of("moves"), Some("d"));
    run.tick();
    assert_eq!(
        run.state_of("moves"),
        Some("d"),
        "travel stops at its target"
    );
    run.travel("moves", "a").tick();
    assert_eq!(run.state_of("moves"), Some("a"));
    // a → h: a→f→h and a→e→h tie on length and priority; a→f comes first in the file.
    run.travel("moves", "h").tick();
    assert_eq!(run.state_of("moves"), Some("f"));
    run.tick();
    assert_eq!(run.state_of("moves"), Some("h"));
    // h → y: through a and z.
    run.travel("moves", "y").ticks(3);
    assert_eq!(run.state_of("moves"), Some("y"));
    // y has no way out: travel teleports, as Godot's does.
    run.travel("moves", "b").tick();
    assert_eq!(run.state_of("moves"), Some("b"));
    run.expect_x(3.0);
    // An unknown state is ignored.
    run.travel("moves", "nowhere").tick();
    assert_eq!(run.state_of("moves"), Some("b"));
}

/// `a` (hip at 1) and `b` (hip at 3); `go` fades a→b over 0.5 s, `back` fades b→a smoothly.
fn fading_graph() -> AnimGraph {
    graph(
        &[
            ("go", Parameter::Bool(false)),
            ("back", Parameter::Bool(false)),
        ],
        vec![
            ("one", clip_node("one", true)),
            ("three", clip_node("three", true)),
            (
                "moves",
                machine(
                    &[("a", "one"), ("b", "three")],
                    "a",
                    vec![
                        StateTransition {
                            xfade: 0.5,
                            ..auto("a", "b", Some("go"))
                        },
                        StateTransition {
                            xfade: 0.5,
                            curve: FadeCurve::Smooth,
                            ..auto("b", "a", Some("back"))
                        },
                    ],
                ),
            ),
        ],
        "moves",
    )
}

#[test]
fn transition_xfade_blends_over_configured_time() {
    let mut run = Run::new(&fading_graph());
    run.tick().expect_x(1.0);
    run.flag("go", true);
    for x in [1.5, 2.0, 2.5, 3.0, 3.0] {
        run.tick().expect_x(x);
    }
    assert_eq!(run.state_of("moves"), Some("b"));
    // Smoothstep over 0.5 s: at 0.125 s the weight is 3(¼)² − 2(¼)³ = 0.15625.
    run.flag("go", false).flag("back", true).tick();
    run.expect_x(3.0 - 2.0 * 0.156_25);

    // A transition taken mid-fade interrupts it: the old fade's source is dropped (Godot 4).
    let mut run = Run::new(&fading_graph());
    run.flag("go", true).ticks(2).expect_x(2.0);
    run.flag("go", false).flag("back", true).tick();
    assert_eq!(run.state_of("moves"), Some("a"));
    run.expect_x(3.0 - 2.0 * 0.156_25);
}

fn at_end_graph(looping: bool, switch: SwitchMode) -> AnimGraph {
    graph(
        &[],
        vec![
            ("short", clip_node("short", looping)),
            ("ten", clip_node("ten", true)),
            (
                "moves",
                machine(
                    &[("a", "short"), ("b", "ten")],
                    "a",
                    vec![StateTransition {
                        switch,
                        ..auto("a", "b", None)
                    }],
                ),
            ),
        ],
        "moves",
    )
}

#[test]
fn at_end_switch_mode_waits_for_clip_end() {
    // `short` is a 0.5 s ramp: hip x shows its time.
    let mut run = Run::new(&at_end_graph(false, SwitchMode::AtEnd));
    for x in [0.125, 0.25, 0.375, 0.5] {
        run.tick().expect_x(x);
        assert_eq!(run.state_of("moves"), Some("a"));
    }
    run.tick().expect_x(10.0);
    assert_eq!(run.state_of("moves"), Some("b"));

    // A looping state ends when it wraps.
    let mut run = Run::new(&at_end_graph(true, SwitchMode::AtEnd));
    for x in [0.125, 0.25, 0.375, 0.0] {
        run.tick().expect_x(x);
    }
    assert_eq!(run.state_of("moves"), Some("a"));
    run.tick().expect_x(10.0);

    // Immediate doesn't wait.
    let mut run = Run::new(&at_end_graph(false, SwitchMode::Immediate));
    run.tick().expect_x(10.0);
}

#[test]
fn sync_switch_mode_seeks_to_the_current_position() {
    let g = |switch: SwitchMode, reset: bool| {
        graph(
            &[("go", Parameter::Bool(false))],
            vec![
                ("ramp", clip_node("ramp", true)),
                ("ramp100", clip_node("ramp100", true)),
                (
                    "moves",
                    machine(
                        &[("a", "ramp"), ("b", "ramp100")],
                        "a",
                        vec![
                            StateTransition {
                                switch,
                                reset,
                                ..auto("a", "b", Some("go"))
                            },
                            auto("b", "a", Some("back")),
                        ],
                    ),
                ),
            ],
            "moves",
        )
    };
    let with_back = |mut g: AnimGraph| {
        g.parameters.insert("back".into(), Parameter::Bool(false));
        g
    };
    // Sync: b starts at a's position (0.375), then advances this tick.
    let mut run = Run::new(&with_back(g(SwitchMode::Sync, true)));
    run.ticks(3).expect_x(0.375);
    run.flag("go", true).tick().expect_x(100.5);
    // Immediate with reset starts b from 0.
    let mut run = Run::new(&with_back(g(SwitchMode::Immediate, true)));
    run.ticks(3).flag("go", true).tick().expect_x(100.125);
    // Without reset, b continues from where it was: a second visit shows it.
    let mut run = Run::new(&with_back(g(SwitchMode::Immediate, false)));
    run.flag("go", true).ticks(2).expect_x(100.25);
    run.flag("go", false)
        .flag("back", true)
        .tick()
        .expect_x(0.125);
    run.flag("back", false)
        .flag("go", true)
        .tick()
        .expect_x(100.375);
}

#[test]
fn transition_allowed_is_not_transition_taken() {
    let g = graph(
        &[
            ("go", Parameter::Bool(true)),
            ("never", Parameter::Bool(false)),
        ],
        vec![
            ("one", clip_node("one", true)),
            ("three", clip_node("three", true)),
            ("five", clip_node("five", true)),
            ("ten", clip_node("ten", true)),
            (
                "moves",
                machine(
                    &[("a", "one"), ("b", "three"), ("c", "five"), ("d", "ten")],
                    "a",
                    vec![
                        StateTransition {
                            condition: Some("go".into()),
                            ..transition("a", "b")
                        },
                        auto("a", "c", Some("never")),
                        StateTransition {
                            advance: AdvanceMode::Disabled,
                            ..auto("a", "d", None)
                        },
                        transition("b", "d"),
                        transition("c", "a"),
                        transition("d", "a"),
                    ],
                ),
            ),
        ],
        "moves",
    );
    let mut run = Run::new(&g);
    run.ticks(5).expect_x(1.0);
    assert_eq!(
        run.state_of("moves"),
        Some("a"),
        "an enabled transition with a true condition is only for travel, and a disabled \
         auto transition is never taken"
    );
    // Travel to d avoids the disabled a→d.
    run.travel("moves", "d").tick();
    assert_eq!(run.state_of("moves"), Some("b"));
    run.tick();
    assert_eq!(run.state_of("moves"), Some("d"));
    run.travel("moves", "a").tick();
    run.flag("never", true).tick();
    assert_eq!(
        run.state_of("moves"),
        Some("c"),
        "an auto transition whose condition holds"
    );
}

#[test]
fn triggers_and_travel_requests_clear_after_every_evaluation() {
    let g = graph(
        &[("jump", Parameter::Trigger)],
        vec![
            ("short", clip_node("short", false)),
            ("one", clip_node("one", true)),
            ("three", clip_node("three", true)),
            (
                "moves",
                machine(
                    &[("a", "short"), ("b", "one"), ("c", "three")],
                    "a",
                    vec![
                        StateTransition {
                            switch: SwitchMode::AtEnd,
                            ..auto("a", "b", Some("jump"))
                        },
                        auto("b", "c", Some("jump")),
                        transition("c", "a"),
                    ],
                ),
            ),
        ],
        "moves",
    );
    let mut run = Run::new(&g);
    // Fired before `a` ends: not taken, and gone by the time `a` ends.
    run.fire("jump").tick();
    assert!(run.player.triggers.is_empty());
    run.ticks(6);
    assert_eq!(
        run.state_of("moves"),
        Some("a"),
        "a cleared trigger never fires later"
    );
    run.fire("jump").tick();
    assert_eq!(run.state_of("moves"), Some("b"));
    run.ticks(3);
    assert_eq!(
        run.state_of("moves"),
        Some("b"),
        "one trigger takes one transition"
    );
    run.fire("jump").fire("unknown").tick();
    assert_eq!(run.state_of("moves"), Some("c"));
    assert!(run.player.triggers.is_empty());
    run.travel("nobody", "a").tick();
    assert!(
        run.player.travel.is_empty(),
        "requests for unknown machines clear too"
    );
}

#[test]
fn invalid_graph_errors_name_the_node() {
    let unknown_clip = graph(&[], vec![("walk", clip_node("missing", true))], "walk");
    let e = bind(&unknown_clip).unwrap_err().to_string();
    assert!(e.contains("\"walk\"") && e.contains("missing"), "{e}");

    let unknown_state = graph(
        &[],
        vec![
            ("one", clip_node("one", true)),
            (
                "moves",
                machine(&[("a", "one")], "a", vec![transition("a", "nowhere")]),
            ),
        ],
        "moves",
    );
    let e = bind(&unknown_state).unwrap_err().to_string();
    assert!(e.contains("\"moves\"") && e.contains("nowhere"), "{e}");

    let cycle = graph(
        &[("t", Parameter::Float(1.0))],
        vec![
            (
                "x",
                AnimNode::TimeScale {
                    input: "y".into(),
                    scale: "t".into(),
                },
            ),
            (
                "y",
                AnimNode::TimeScale {
                    input: "x".into(),
                    scale: "t".into(),
                },
            ),
        ],
        "x",
    );
    let e = bind(&cycle).unwrap_err().to_string();
    assert!(e.contains("cycle") && e.contains("x -> y -> x"), "{e}");
}

#[test]
fn filter_bones_the_skeleton_lacks_are_reported() {
    let bound = bind(&one_shot(FadeCurve::Linear, &["arm", "tail", "wing"])).unwrap();
    assert_eq!(bound.unmatched(), ["tail", "wing"]);
}

#[test]
fn a_state_of_the_wrong_size_is_reset() {
    let mut run = Run::new(&fading_graph());
    run.flag("go", true).ticks(6);
    assert_eq!(run.state_of("moves"), Some("b"));
    run.state = AnimGraphState::default();
    run.flag("go", false).tick().expect_x(1.0);
    assert_eq!(
        run.state_of("moves"),
        Some("a"),
        "an empty state restarts the graph"
    );
}

#[test]
fn a_corrupt_state_is_reset_not_trusted() {
    let g = graph(
        &[("pick", Parameter::Int(0))],
        vec![
            ("ra", clip_node("ramp", true)),
            (
                "root",
                AnimNode::Transition {
                    inputs: vec![TransitionInput {
                        node: "ra".into(),
                        reset: true,
                    }],
                    select: "pick".into(),
                    xfade: 0.25,
                    curve: FadeCurve::Linear,
                },
            ),
        ],
        "root",
    );
    let mut run = Run::new(&g);
    run.ticks(2).expect_x(0.25);
    run.state.nodes[1].current = 99;
    run.state.nodes[1].previous = Some(7);
    run.state.nodes[0].time = f32::NAN;
    run.tick().expect_x(0.125);
    assert_eq!(run.state.nodes[1].current, 0);
    assert_eq!(run.state.nodes[1].previous, None);

    let mut run = Run::new(&travel_graph());
    // Nodes in name order: a_clip..h_clip (0–7), moves (8), y_clip, z_clip.
    let moves = 8;
    run.state.nodes[moves].path = vec![500];
    run.state.nodes[moves].via = Some(400);
    run.tick();
    assert_eq!(run.state_of("moves"), Some("a"));
}
