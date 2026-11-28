//! The node library (M4 4.9, D6, H4): a built-in core, `#[graph_node]` functions, and nodes
//! generated from the engine's registry (component get/set, signal raise/on).

use std::collections::BTreeMap;

use concordia_reflect::{GraphFnInfo, Shape, TypeRegistration, TypeRegistry, is_signal};
use serde::Serialize;

use crate::types::{GenericKind, PinType};

/// A typed data pin of a node.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PinDef {
    /// Name.
    pub name: String,
    /// Type.
    pub ty: PinType,
    /// Must be linked or given a value.
    pub required: bool,
}

/// A node the library offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeDef {
    /// Stable operation id (what `.graph.ron` names).
    pub op: String,
    /// Library category.
    pub category: String,
    /// What it does.
    pub docs: String,
    /// No side effects and no control flow (callable from pure graphs).
    pub pure: bool,
    /// An entry point (an event: control starts here).
    pub event: bool,
    /// Its pins depend on the graph (`graph.input`, `graph.output`).
    pub dynamic: bool,
    /// Exec inputs.
    pub exec_in: Vec<String>,
    /// Exec outputs.
    pub exec_out: Vec<String>,
    /// Data inputs.
    pub inputs: Vec<PinDef>,
    /// Data outputs.
    pub outputs: Vec<PinDef>,
}

impl NodeDef {
    /// Data input `name`.
    pub fn input(&self, name: &str) -> Option<&PinDef> {
        self.inputs.iter().find(|p| p.name == name)
    }

    /// Data output `name`.
    pub fn output(&self, name: &str) -> Option<&PinDef> {
        self.outputs.iter().find(|p| p.name == name)
    }
}

/// Every node a graph can use, by op.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeLibrary {
    nodes: BTreeMap<String, NodeDef>,
}

impl NodeLibrary {
    /// The built-in core and the `#[graph_node]` functions.
    pub fn core() -> Self {
        let mut lib = Self::default();
        for def in core_nodes() {
            lib.add(def);
        }
        for info in crate::fns::graph_fns() {
            lib.add(from_fn(&info));
        }
        lib
    }

    /// The core plus nodes for every registration `is_component` accepts: `raise.`/`on.` for
    /// signals, `get.`/`set.` (and field getters) for other components.
    pub fn build(
        registry: &TypeRegistry,
        is_component: impl Fn(&TypeRegistration) -> bool,
    ) -> Self {
        let mut lib = Self::core();
        for name in registry.names() {
            let Some(r) = registry.get_by_name(name) else {
                continue;
            };
            if !is_component(r) {
                continue;
            }
            let defs = match is_signal(r) {
                true => signal_nodes(r),
                false => component_nodes(r),
            };
            for def in defs {
                lib.add(def);
            }
        }
        lib
    }

    /// Adds (or replaces) a node.
    pub fn add(&mut self, def: NodeDef) {
        self.nodes.insert(def.op.clone(), def);
    }

    /// The node `op`.
    pub fn get(&self, op: &str) -> Option<&NodeDef> {
        self.nodes.get(op)
    }

    /// Every op, sorted.
    pub fn ops(&self) -> impl Iterator<Item = &str> {
        self.nodes.keys().map(String::as_str)
    }

    /// Every node, sorted by op.
    pub fn nodes(&self) -> impl Iterator<Item = &NodeDef> {
        self.nodes.values()
    }

    /// The library as JSON (what `graph.list_nodes` returns).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.nodes).unwrap_or_default()
    }
}

fn pin(name: &str, ty: PinType, required: bool) -> PinDef {
    PinDef {
        name: name.to_owned(),
        ty,
        required,
    }
}

pub(crate) fn req(name: &str, ty: PinType) -> PinDef {
    pin(name, ty, true)
}

pub(crate) fn out(name: &str, ty: PinType) -> PinDef {
    pin(name, ty, false)
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// A pure data node.
pub(crate) fn pure(
    op: &str,
    category: &str,
    docs: &str,
    inputs: Vec<PinDef>,
    outputs: Vec<PinDef>,
) -> NodeDef {
    NodeDef {
        op: op.to_owned(),
        category: category.to_owned(),
        docs: docs.to_owned(),
        pure: true,
        event: false,
        dynamic: false,
        exec_in: Vec::new(),
        exec_out: Vec::new(),
        inputs,
        outputs,
    }
}

/// A node with control flow.
pub(crate) fn flow(
    op: &str,
    category: &str,
    docs: &str,
    (exec_in, exec_out): (&[&str], &[&str]),
    inputs: Vec<PinDef>,
    outputs: Vec<PinDef>,
) -> NodeDef {
    NodeDef {
        pure: false,
        exec_in: names(exec_in),
        exec_out: names(exec_out),
        ..pure(op, category, docs, inputs, outputs)
    }
}

/// An entry point.
fn event(op: &str, category: &str, docs: &str, exec_out: &[&str], outputs: Vec<PinDef>) -> NodeDef {
    NodeDef {
        event: true,
        ..flow(op, category, docs, (&[], exec_out), Vec::new(), outputs)
    }
}

fn core_nodes() -> Vec<NodeDef> {
    use PinType::*;
    let num = || Generic(GenericKind::Numeric);
    let scalar = || Generic(GenericKind::Scalar);
    let mut nodes = vec![
        event(
            "event.on_tick",
            "Events",
            "Runs every simulation tick for each entity the graph runs on.",
            &["out"],
            vec![out("entity", EntityId), out("dt", F32)],
        ),
        flow(
            "branch",
            "Flow",
            "Continues on `true` or `false`.",
            (&["in"], &["true", "false"]),
            vec![req("cond", Bool)],
            vec![],
        ),
        flow(
            "sequence",
            "Flow",
            "Runs `then0`, `then1` and `then2` in order.",
            (&["in"], &["then0", "then1", "then2"]),
            vec![],
            vec![],
        ),
        flow(
            "for_each",
            "Flow",
            "Runs `body` once per entity of `list`, then `done`.",
            (&["in"], &["body", "done"]),
            vec![req("list", List(Box::new(EntityId)))],
            vec![out("item", EntityId)],
        ),
        flow(
            "gate",
            "Flow",
            "Continues only while `open`.",
            (&["in"], &["out"]),
            vec![req("open", Bool)],
            vec![],
        ),
        flow(
            "query.each",
            "Queries",
            "Runs `body` once per entity that has every component in `with` and none in `without` (by name), then `done`.",
            (&["in"], &["body", "done"]),
            vec![
                req("with", List(Box::new(String))),
                pin("without", List(Box::new(String)), false),
            ],
            vec![out("entity", EntityId)],
        ),
        pure(
            "entity.self",
            "Entities",
            "The entity the graph runs on.",
            vec![],
            vec![out("entity", EntityId)],
        ),
        pure(
            "math.saturate",
            "Math",
            "Clamps to [0, 1].",
            vec![req("x", F32)],
            vec![out("out", F32)],
        ),
        pure(
            "math.lerp",
            "Math",
            "`a` + (`b` - `a`) × `t`.",
            vec![req("a", num()), req("b", num()), req("t", F32)],
            vec![out("out", num())],
        ),
        pure(
            "math.clamp",
            "Math",
            "`x` limited to [`lo`, `hi`].",
            vec![req("x", scalar()), req("lo", scalar()), req("hi", scalar())],
            vec![out("out", scalar())],
        ),
        pure(
            "math.neg",
            "Math",
            "-`x`.",
            vec![req("x", num())],
            vec![out("out", num())],
        ),
        pure(
            "math.abs",
            "Math",
            "|`x`|.",
            vec![req("x", scalar())],
            vec![out("out", scalar())],
        ),
        pure(
            "logic.not",
            "Logic",
            "Not `x`.",
            vec![req("x", Bool)],
            vec![out("out", Bool)],
        ),
        pure(
            "conv.i32_to_f32",
            "Conversions",
            "An integer as a number.",
            vec![req("x", I32)],
            vec![out("out", F32)],
        ),
        pure(
            "conv.f32_to_i32",
            "Conversions",
            "A number truncated toward zero.",
            vec![req("x", F32)],
            vec![out("out", I32)],
        ),
        pure(
            "conv.vec3",
            "Conversions",
            "A vector from its components.",
            vec![req("x", F32), req("y", F32), req("z", F32)],
            vec![out("out", Vec3)],
        ),
        pure(
            "conv.split_vec3",
            "Conversions",
            "A vector's components.",
            vec![req("v", Vec3)],
            vec![out("x", F32), out("y", F32), out("z", F32)],
        ),
        NodeDef {
            dynamic: true,
            ..pure(
                "graph.input",
                "Graph",
                "The graph's inputs (its signature).",
                vec![],
                vec![],
            )
        },
        NodeDef {
            dynamic: true,
            pure: false,
            ..pure(
                "graph.output",
                "Graph",
                "The graph's outputs (its signature).",
                vec![],
                vec![],
            )
        },
        event(
            "behavior.root",
            "Behavior",
            "A behavior tree's root.",
            &["child"],
            vec![],
        ),
        flow(
            "behavior.sequence",
            "Behavior",
            "Runs children in order until one fails.",
            (&["in"], &["child0", "child1", "child2", "child3"]),
            vec![],
            vec![],
        ),
        flow(
            "behavior.selector",
            "Behavior",
            "Runs children in order until one succeeds.",
            (&["in"], &["child0", "child1", "child2", "child3"]),
            vec![],
            vec![],
        ),
        flow(
            "behavior.succeed",
            "Behavior",
            "Succeeds at once.",
            (&["in"], &[]),
            vec![],
            vec![],
        ),
        pure(
            "utility.curve",
            "Utility",
            "A response curve of `x` (`linear`, `quadratic`, `logistic`, `inverse`), always in [0, 1].",
            vec![req("x", F32), pin("curve", String, false)],
            vec![out("out", F32)],
        ),
    ];
    nodes.extend(behavior_nodes());
    for (op, docs) in [
        ("add", "`a` + `b`."),
        ("sub", "`a` - `b`."),
        ("mul", "`a` × `b`."),
        ("div", "`a` ÷ `b`."),
    ] {
        nodes.push(pure(
            &format!("math.{op}"),
            "Math",
            docs,
            vec![req("a", num()), req("b", num())],
            vec![out("out", num())],
        ));
    }
    for (op, docs) in [("min", "The smaller."), ("max", "The larger.")] {
        nodes.push(pure(
            &format!("math.{op}"),
            "Math",
            docs,
            vec![req("a", scalar()), req("b", scalar())],
            vec![out("out", scalar())],
        ));
    }
    for (op, docs) in [
        ("gt", "`a` > `b`."),
        ("ge", "`a` ≥ `b`."),
        ("lt", "`a` < `b`."),
        ("le", "`a` ≤ `b`."),
        ("eq", "`a` = `b`."),
        ("ne", "`a` ≠ `b`."),
    ] {
        nodes.push(pure(
            &format!("math.{op}"),
            "Math",
            docs,
            vec![req("a", scalar()), req("b", scalar())],
            vec![out("out", Bool)],
        ));
    }
    for (op, docs) in [("and", "`a` and `b`."), ("or", "`a` or `b`.")] {
        nodes.push(pure(
            &format!("logic.{op}"),
            "Logic",
            docs,
            vec![req("a", Bool), req("b", Bool)],
            vec![out("out", Bool)],
        ));
    }
    nodes
}

/// The behavior-tree node set beyond root, sequence, selector and succeed (M4 4.12e): composites
/// (`child0`–`child3`), decorators (one `child`) and tasks, parameters as constant inputs.
fn behavior_nodes() -> Vec<NodeDef> {
    use PinType::{F32, I32, String};
    let children: &[&str] = &["child0", "child1", "child2", "child3"];
    let composite = |op: &str, docs: &str, inputs| {
        flow(op, "Behavior", docs, (&["in"], children), inputs, vec![])
    };
    let decorator = |op: &str, docs: &str, inputs| {
        flow(op, "Behavior", docs, (&["in"], &["child"]), inputs, vec![])
    };
    let task =
        |op: &str, docs: &str, inputs| flow(op, "Behavior", docs, (&["in"], &[]), inputs, vec![]);
    let text = |name: &str| req(name, String);
    let opt_text = |name: &str| pin(name, String, false);
    vec![
        composite(
            "behavior.parallel",
            "Runs every child at once; `policy` `all` (default: succeed when all do) or `one`.",
            vec![opt_text("policy")],
        ),
        composite(
            "behavior.random",
            "Runs one child picked at random (seeded); `weights` like `1, 2, 1` (default: equal).",
            vec![opt_text("weights")],
        ),
        decorator(
            "behavior.condition",
            "Runs its child while blackboard `key` compares (`op`: eq, ne, lt, le, gt, ge, set, unset) with `value`; `abort`: none, self or lower.",
            vec![
                text("key"),
                opt_text("op"),
                opt_text("value"),
                opt_text("abort"),
            ],
        ),
        decorator(
            "behavior.cooldown",
            "Fails for `seconds` after its child finished.",
            vec![req("seconds", F32)],
        ),
        decorator(
            "behavior.loop",
            "Runs its child `count` times (0 or absent: forever) while it succeeds.",
            vec![pin("count", I32, false)],
        ),
        decorator(
            "behavior.time_limit",
            "Fails its child after `seconds`.",
            vec![req("seconds", F32)],
        ),
        decorator(
            "behavior.inverter",
            "Swaps its child's success and failure.",
            vec![],
        ),
        task(
            "behavior.move_to",
            "Walks to the entity or point in blackboard `key`.",
            vec![text("key")],
        ),
        task(
            "behavior.wait",
            "Waits `seconds`.",
            vec![req("seconds", F32)],
        ),
        task(
            "behavior.play_anim",
            "Fires an animation-graph `trigger`.",
            vec![text("trigger")],
        ),
        task(
            "behavior.face",
            "Turns toward the entity or point in blackboard `key`.",
            vec![text("key")],
        ),
        task(
            "behavior.set_key",
            "Writes `value` (text, parsed by the key's type) to blackboard `key`.",
            vec![text("key"), text("value")],
        ),
        task(
            "behavior.cast",
            "Casts `skill` at the entity in blackboard `target_key` (raises `ai.Cast`).",
            vec![text("skill"), text("target_key")],
        ),
        task(
            "behavior.run_graph",
            "Asks graph module `graph` to run (raises `ai.Run`).",
            vec![text("graph")],
        ),
        task(
            "behavior.plan",
            "Asks the game's LLM (opt-in) to choose one of `options` (comma-separated) for `goal` into Text `key` within `deadline_ticks` (default 60); fails otherwise.",
            vec![
                text("goal"),
                text("options"),
                text("key"),
                pin("deadline_ticks", I32, false),
            ],
        ),
        task("behavior.fail", "Fails at once.", vec![]),
    ]
}

/// A `#[graph_node]` function as a node: `<category>.<name>`, output `out`.
fn from_fn(info: &GraphFnInfo) -> NodeDef {
    let op = format!("{}.{}", info.category.to_lowercase(), info.name);
    let inputs = info
        .inputs
        .iter()
        .map(|(name, ty)| req(name, PinType::of(ty)))
        .collect();
    let outputs = info
        .output
        .iter()
        .map(|ty| out("out", PinType::of(ty)))
        .collect();
    match info.pure {
        true => pure(&op, info.category, info.docs, inputs, outputs),
        false => flow(
            &op,
            info.category,
            info.docs,
            (&["in"], &["out"]),
            inputs,
            outputs,
        ),
    }
}

/// `get.<C>`, `set.<C>` and a getter and a setter per struct field.
fn component_nodes(r: &TypeRegistration) -> Vec<NodeDef> {
    let info = r.info();
    let name = info.name;
    let ty = PinType::Named(name.to_owned());
    let category = info.category.unwrap_or("Components");
    let entity = || pin("entity", PinType::EntityId, false);
    let mut nodes = vec![
        pure(
            &format!("get.{name}"),
            category,
            &format!("`{name}` of `entity` (default: the graph's entity)."),
            vec![entity()],
            vec![out("value", ty.clone())],
        ),
        flow(
            &format!("set.{name}"),
            category,
            &format!("Sets `{name}` on `entity` (default: the graph's entity)."),
            (&["in"], &["out"]),
            vec![entity(), req("value", ty)],
            vec![],
        ),
    ];
    for (field, ty, docs) in fields(r) {
        nodes.push(pure(
            &format!("get.{name}.{field}"),
            category,
            docs,
            vec![entity()],
            vec![out("value", ty.clone())],
        ));
        nodes.push(flow(
            &format!("set.{name}.{field}"),
            category,
            &format!("Sets `{name}.{field}` on `entity` (default: the graph's entity)."),
            (&["in"], &["out"]),
            vec![entity(), req("value", ty)],
            vec![],
        ));
    }
    nodes
}

/// A signal field's pin: its name, or `field_<name>` where that is a fixed pin's name.
pub(crate) fn field_pin(field: &str) -> String {
    match ["target", "entity", "signal"].contains(&field) {
        true => format!("field_{field}"),
        false => field.to_owned(),
    }
}

/// A struct's fields: (name, pin type, docs); none for other shapes.
fn fields(r: &TypeRegistration) -> Vec<(&'static str, PinType, &'static str)> {
    match &r.info().shape {
        Shape::Struct { fields, .. } => fields
            .iter()
            .map(|f| (f.name, PinType::of(&f.ty), f.docs))
            .collect(),
        _ => Vec::new(),
    }
}

/// `raise.<S>` (from the signal's fields) and `on.<S>` (with them).
fn signal_nodes(r: &TypeRegistration) -> Vec<NodeDef> {
    let info = r.info();
    let name = info.name;
    let ty = PinType::Named(name.to_owned());
    let category = info.category.unwrap_or("Signals");
    let mut raise_inputs = vec![req("target", PinType::EntityId)];
    let mut on_outputs = vec![out("entity", PinType::EntityId), out("signal", ty)];
    for (field, ty, _) in fields(r) {
        raise_inputs.push(req(&field_pin(field), ty.clone()));
        on_outputs.push(out(&field_pin(field), ty));
    }
    vec![
        flow(
            &format!("raise.{name}"),
            category,
            &format!("Raises `{name}` on `target` (delivered next tick)."),
            (&["in"], &["out"]),
            raise_inputs,
            vec![],
        ),
        event(
            &format!("on.{name}"),
            category,
            &format!("Runs when an entity receives `{name}`."),
            &["out"],
            on_outputs,
        ),
    ]
}
