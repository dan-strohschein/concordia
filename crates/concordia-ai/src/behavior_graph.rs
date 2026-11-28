//! Behavior graphs to behavior trees (M4 4.12e, M10): a checked `.graph.ron` of kind Behavior
//! becomes the flat `ai.BehaviorTree` the runtime ticks. Graph variables are the blackboard keys;
//! node parameters are constant inputs; children follow the exec pins in order.

use std::collections::{BTreeMap, BTreeSet};

use concordia_graph::{
    Code, Diagnostic, Graph, GraphKind, Input, Literal, Node, NodeLibrary, check,
};

use crate::{
    Abort, BbType, BbValue, BehaviorTree, CmpOp, NodeKind, ParallelPolicy, Spec, TaskKind,
};

/// Checks and compiles a Behavior graph.
pub fn compile_behavior(graph: &Graph, lib: &NodeLibrary) -> Result<BehaviorTree, Vec<Diagnostic>> {
    let report = check(graph, lib);
    if !report.is_ok() {
        return Err(report.diagnostics);
    }
    if graph.kind != GraphKind::Behavior {
        return Err(vec![diag(
            None,
            None,
            "only Behavior graphs compile to behavior trees".into(),
        )]);
    }
    let keys = keys(graph)?;
    let root = graph
        .nodes
        .iter()
        .find(|(_, n)| n.op == "behavior.root")
        .map(|(id, _)| id.as_str());
    let Some(child) = root.and_then(|r| children(graph, r, &["child"]).into_iter().next()) else {
        return Err(vec![diag(
            root,
            Some("child"),
            "the root has no child".into(),
        )]);
    };
    let mut c = Compiler {
        graph,
        keys: &keys,
        visiting: BTreeSet::new(),
    };
    let spec = c.spec(child).map_err(|d| vec![d])?;
    BehaviorTree::build(keys, spec).map_err(|e| vec![diag(None, None, e.to_string())])
}

fn diag(node: Option<&str>, pin: Option<&str>, message: String) -> Diagnostic {
    Diagnostic {
        code: Code::unsupported_node,
        node_id: node.map(str::to_owned),
        pin: pin.map(str::to_owned),
        message,
        suggestion: None,
    }
}

/// The graph's variables as blackboard keys.
fn keys(graph: &Graph) -> Result<BTreeMap<String, BbType>, Vec<Diagnostic>> {
    let mut keys = BTreeMap::new();
    let mut errors = Vec::new();
    for v in &graph.variables {
        let ty = match v.ty.as_str() {
            "bool" => BbType::Bool,
            "i32" | "i64" => BbType::Int,
            "f32" => BbType::Float,
            "Vec3" => BbType::Vec3,
            "EntityId" => BbType::Entity,
            "String" => BbType::Text,
            other => {
                errors.push(diag(None, Some(&v.name), format!("variable `{}` is a {other}; blackboard keys are bool, i32, i64, f32, Vec3, EntityId or String", v.name)));
                continue;
            }
        };
        keys.insert(v.name.clone(), ty);
    }
    match errors.is_empty() {
        true => Ok(keys),
        false => Err(errors),
    }
}

/// The nodes linked from `node`'s exec outputs `pins`, in pin order.
fn children<'g>(graph: &'g Graph, node: &str, pins: &[&str]) -> Vec<&'g str> {
    pins.iter()
        .flat_map(|pin| {
            graph
                .exec
                .iter()
                .filter(move |e| e.0 == node && e.1 == *pin)
                .map(|e| e.2.as_str())
        })
        .collect()
}

struct Compiler<'g> {
    graph: &'g Graph,
    keys: &'g BTreeMap<String, BbType>,
    visiting: BTreeSet<String>,
}

const CHILDREN: [&str; 4] = ["child0", "child1", "child2", "child3"];

impl Compiler<'_> {
    fn spec(&mut self, id: &str) -> Result<Spec, Diagnostic> {
        if !self.visiting.insert(id.to_owned()) {
            return Err(diag(
                Some(id),
                None,
                "a behavior tree can't loop back on itself".into(),
            ));
        }
        let node = &self.graph.nodes[id];
        let p = Params {
            id,
            node,
            keys: self.keys,
        };
        let (kind, pins): (NodeKind, &[&str]) = match node.op.as_str() {
            "behavior.sequence" => (NodeKind::Sequence, &CHILDREN),
            "behavior.selector" => (NodeKind::Selector, &CHILDREN),
            "behavior.parallel" => (
                NodeKind::Parallel {
                    policy: p.policy()?,
                },
                &CHILDREN,
            ),
            "behavior.random" => (
                NodeKind::RandomSelector {
                    weights: Vec::new(),
                },
                &CHILDREN,
            ),
            "behavior.condition" => (p.condition()?, &["child"]),
            "behavior.cooldown" => (
                NodeKind::Cooldown {
                    seconds: p.float("seconds")?,
                },
                &["child"],
            ),
            "behavior.loop" => (
                NodeKind::Loop {
                    count: p.int("count").unwrap_or(0).max(0) as u32,
                },
                &["child"],
            ),
            "behavior.time_limit" => (
                NodeKind::TimeLimit {
                    seconds: p.float("seconds")?,
                },
                &["child"],
            ),
            "behavior.inverter" => (NodeKind::Inverter, &["child"]),
            _ => (NodeKind::Task(p.task()?), &[]),
        };
        let mut children = Vec::new();
        for child in self::children(self.graph, id, pins) {
            children.push(self.spec(child)?);
        }
        self.visiting.remove(id);
        let kind = match kind {
            NodeKind::RandomSelector { .. } => NodeKind::RandomSelector {
                weights: p.weights(children.len())?,
            },
            other => other,
        };
        Ok(Spec::new(kind).children(children))
    }
}

/// A node's constant parameters.
struct Params<'a> {
    id: &'a str,
    node: &'a Node,
    keys: &'a BTreeMap<String, BbType>,
}

impl Params<'_> {
    fn fail<T>(&self, pin: &str, message: String) -> Result<T, Diagnostic> {
        Err(diag(Some(self.id), Some(pin), message))
    }

    fn literal(&self, pin: &str) -> Option<&Literal> {
        match self.node.inputs.get(pin) {
            Some(Input::Value(v)) => Some(v),
            _ => None,
        }
    }

    fn text(&self, pin: &str) -> Result<String, Diagnostic> {
        match self.literal(pin) {
            Some(Literal::Text(t)) => Ok(t.clone()),
            _ => self.fail(pin, format!("`{pin}` must be a constant text")),
        }
    }

    fn opt_text(&self, pin: &str) -> Option<String> {
        self.text(pin).ok()
    }

    fn float(&self, pin: &str) -> Result<f32, Diagnostic> {
        match self.literal(pin) {
            Some(Literal::Float(f)) => Ok(*f as f32),
            Some(Literal::Int(i)) => Ok(*i as f32),
            _ => self.fail(pin, format!("`{pin}` must be a constant number")),
        }
    }

    fn int(&self, pin: &str) -> Result<i64, Diagnostic> {
        match self.literal(pin) {
            Some(Literal::Int(i)) => Ok(*i),
            _ => self.fail(pin, format!("`{pin}` must be a constant integer")),
        }
    }

    /// A key the graph declares.
    fn key(&self, pin: &str) -> Result<(String, BbType), Diagnostic> {
        let key = self.text(pin)?;
        match self.keys.get(&key) {
            Some(ty) => Ok((key, *ty)),
            None => self.fail(
                pin,
                format!("`{key}` isn't a graph variable (the blackboard's keys are its variables)"),
            ),
        }
    }

    /// `text` as a value of `ty`.
    fn value(&self, pin: &str, text: &str, ty: BbType) -> Result<BbValue, Diagnostic> {
        let t = text.trim();
        let bad = || {
            diag(
                Some(self.id),
                Some(pin),
                format!("{t:?} isn't a {ty:?} value"),
            )
        };
        Ok(match ty {
            BbType::Bool => BbValue::Bool(t.parse().map_err(|_| bad())?),
            BbType::Int => BbValue::Int(t.parse().map_err(|_| bad())?),
            BbType::Float => BbValue::Float(t.parse().map_err(|_| bad())?),
            BbType::Text => BbValue::Text(text.to_owned()),
            BbType::Vec3 => {
                let v: Vec<f32> = t
                    .split(',')
                    .map(|x| x.trim().parse())
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad())?;
                BbValue::Vec3(v.try_into().map_err(|_| bad())?)
            }
            BbType::Entity => return Err(bad()),
        })
    }

    fn policy(&self) -> Result<ParallelPolicy, Diagnostic> {
        match self.opt_text("policy").as_deref() {
            None | Some("all") => Ok(ParallelPolicy::All),
            Some("one") => Ok(ParallelPolicy::One),
            Some(other) => self.fail("policy", format!("{other:?}: use `all` or `one`")),
        }
    }

    fn weights(&self, n: usize) -> Result<Vec<f32>, Diagnostic> {
        let Some(text) = self.opt_text("weights") else {
            return Ok(vec![1.0; n]);
        };
        let w: Vec<f32> = text
            .split(',')
            .map(|x| x.trim().parse())
            .collect::<Result<_, _>>()
            .map_err(|_| {
                diag(
                    Some(self.id),
                    Some("weights"),
                    format!("{text:?}: numbers like `1, 2, 1`"),
                )
            })?;
        match w.len() == n {
            true => Ok(w),
            false => self.fail("weights", format!("{} weights for {n} children", w.len())),
        }
    }

    fn condition(&self) -> Result<NodeKind, Diagnostic> {
        let (key, ty) = self.key("key")?;
        let op = match self.opt_text("op").as_deref().unwrap_or("eq") {
            "eq" => CmpOp::Eq,
            "ne" => CmpOp::Ne,
            "lt" => CmpOp::Lt,
            "le" => CmpOp::Le,
            "gt" => CmpOp::Gt,
            "ge" => CmpOp::Ge,
            "set" => CmpOp::IsSet,
            "unset" => CmpOp::NotSet,
            other => {
                return self.fail(
                    "op",
                    format!("{other:?}: use eq, ne, lt, le, gt, ge, set or unset"),
                );
            }
        };
        let value = match (op, self.opt_text("value")) {
            (CmpOp::IsSet | CmpOp::NotSet, _) | (_, None) => BbValue::None,
            (_, Some(text)) => self.value("value", &text, ty)?,
        };
        let abort = match self.opt_text("abort").as_deref().unwrap_or("none") {
            "none" => Abort::None,
            "self" => Abort::SelfOnly,
            "lower" => Abort::Lower,
            other => return self.fail("abort", format!("{other:?}: use none, self or lower")),
        };
        Ok(NodeKind::Condition {
            key,
            op,
            value,
            abort,
        })
    }

    fn task(&self) -> Result<TaskKind, Diagnostic> {
        Ok(match self.node.op.as_str() {
            "behavior.move_to" => TaskKind::MoveTo {
                key: self.key("key")?.0,
            },
            "behavior.wait" => TaskKind::Wait {
                seconds: self.float("seconds")?,
            },
            "behavior.play_anim" => TaskKind::PlayAnim {
                trigger: self.text("trigger")?,
            },
            "behavior.face" => TaskKind::FaceTarget {
                key: self.key("key")?.0,
            },
            "behavior.set_key" => {
                let (key, ty) = self.key("key")?;
                let value = self.value("value", &self.text("value")?, ty)?;
                TaskKind::SetBlackboard { key, value }
            }
            "behavior.cast" => TaskKind::CastSkill {
                skill: self.text("skill")?,
                target_key: self.key("target_key")?.0,
            },
            "behavior.run_graph" => TaskKind::RunGraph {
                graph: self.text("graph")?,
            },
            "behavior.plan" => TaskKind::Plan {
                goal: self.text("goal")?,
                deadline_ticks: self.int("deadline_ticks").unwrap_or(60).max(1) as u32,
                options: self
                    .text("options")?
                    .split(',')
                    .map(|o| o.trim().to_owned())
                    .filter(|o| !o.is_empty())
                    .collect(),
                key: self.key("key")?.0,
            },
            "behavior.succeed" => TaskKind::Succeed,
            "behavior.fail" => TaskKind::Fail,
            other => {
                return Err(diag(
                    Some(self.id),
                    None,
                    format!("`{other}` isn't a behavior-tree node"),
                ));
            }
        })
    }
}
