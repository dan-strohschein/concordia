//! Push notifications: per-session event sinks registered by transports, topic subscriptions,
//! and the events derived from each successful call.

use std::collections::{BTreeMap, BTreeSet};

use concordia_command::{EditOp, Transaction};
use concordia_core::id::SessionId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::Engine;

/// Receives JSON-RPC notification objects for one session.
pub type EventSink = Box<dyn FnMut(Value) + Send>;

/// What a session can subscribe to.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
pub enum Topic {
    /// Committed edit-world changes, undo and redo (with actor and entities).
    edit,
    /// Scene opened, created or saved.
    scene,
    /// Simulation state and tick after `sim.*` calls.
    sim,
    /// Uncommitted edits inside open transactions (`pending: true`), and aborts (`aborted: true`).
    pending,
    /// New log records (`latest`: the newest record's seq); fetch them with `log.tail`.
    log,
    /// Agent Studio activity: task changes, file-edit audits, actor slots (see `studio.activity`).
    studio,
}

/// Sinks and subscriptions by session.
#[derive(Default)]
pub(crate) struct EventHub {
    sinks: BTreeMap<SessionId, EventSink>,
    topics: BTreeMap<SessionId, BTreeSet<Topic>>,
}

impl EventHub {
    pub(crate) fn set_sink(&mut self, session: SessionId, sink: EventSink) {
        self.sinks.insert(session, sink);
    }

    pub(crate) fn has_sink(&self, session: SessionId) -> bool {
        self.sinks.contains_key(&session)
    }

    /// Replaces the session's subscriptions.
    pub(crate) fn subscribe(&mut self, session: SessionId, topics: &[Topic]) {
        self.topics
            .insert(session, topics.iter().copied().collect());
    }

    pub(crate) fn unsubscribe(&mut self, session: SessionId) {
        self.topics.remove(&session);
    }

    pub(crate) fn remove(&mut self, session: SessionId) {
        self.sinks.remove(&session);
        self.topics.remove(&session);
    }

    pub(crate) fn wants(&self, topic: Topic) -> bool {
        self.topics.values().any(|t| t.contains(&topic))
    }

    /// Sends `events.event` with `params` (plus `topic`) to every session subscribed to `topic`.
    pub(crate) fn publish(&mut self, topic: Topic, mut params: Value) {
        params["topic"] = json!(topic);
        let message = json!({"jsonrpc": "2.0", "method": "events.event", "params": params});
        for (session, topics) in &self.topics {
            if topics.contains(&topic)
                && let Some(sink) = self.sinks.get_mut(session)
            {
                sink(message.clone());
            }
        }
    }
}

/// History lengths before a call, to tell commits, undos and redos apart afterwards.
pub(crate) struct Marker {
    /// The log ring's next sequence number.
    log: u64,
    done: usize,
    undone: usize,
    /// The open transaction's id and op count.
    open: Option<(concordia_core::id::TxnId, usize)>,
    /// Entities of the open transaction (only collected before `txn.abort`).
    open_entities: Vec<String>,
}

impl Marker {
    /// The edit log's position before `method` runs.
    pub(crate) fn of(engine: &Engine, method: &str) -> Self {
        let Some(scene) = engine.scene.as_ref() else {
            return Self {
                log: engine.ring.next_seq(),
                done: 0,
                undone: 0,
                open: None,
                open_entities: Vec::new(),
            };
        };
        let open = scene.log.open_txn();
        let open_entities = match (method, open) {
            ("txn.abort", Some(txn)) => entities_of(txn.ops()),
            _ => Vec::new(),
        };
        Self {
            log: engine.ring.next_seq(),
            done: scene.log.done().len(),
            undone: scene.log.undone().len(),
            open: open.map(|t| (t.id, t.op_count())),
            open_entities,
        }
    }
}

/// Publishes the events a successful call caused (changes first, then any log records it wrote).
pub(crate) fn publish_after(engine: &mut Engine, method: &str, before: &Marker, result: &Value) {
    publish_changes(engine, method, before, result);
    publish_log(engine, before);
}

fn publish_changes(engine: &mut Engine, method: &str, before: &Marker, result: &Value) {
    let path = engine.scene.as_ref().map(|s| s.path.clone());
    match method {
        "scene.open" | "scene.new" | "scene.save" if engine.events.wants(Topic::scene) => {
            let event = match method {
                "scene.open" => "opened",
                "scene.new" => "new",
                _ => "saved",
            };
            engine
                .events
                .publish(Topic::scene, json!({"event": event, "path": path}));
        }
        "sim.step" | "sim.play" | "sim.pause" | "sim.stop" if engine.events.wants(Topic::sim) => {
            engine.events.publish(
                Topic::sim,
                json!({"method": method, "tick": result.get("tick"), "state": result.get("state")}),
            );
        }
        "scene.open" | "scene.new" => {}
        _ => {
            publish_edit(engine, method, before);
            if engine.events.wants(Topic::pending) {
                publish_pending(engine, method, before);
            }
        }
    }
}

/// Publishes ops a call added to the open transaction, or the abort that reverted it.
fn publish_pending(engine: &mut Engine, method: &str, before: &Marker) {
    let Some(scene) = engine.scene.as_ref() else {
        return;
    };
    let params = match (scene.log.open_txn(), before.open) {
        (Some(txn), Some((id, ops))) if txn.id == id && txn.op_count() > ops => json!({
            "method": method,
            "txn": txn.id.to_string(),
            "label": txn.label,
            "actor": txn.author.actor.to_string(),
            "entities": entities_of(&txn.ops()[ops..]),
            "pending": true,
        }),
        (Some(txn), None) if txn.op_count() > 0 => json!({
            "method": method,
            "txn": txn.id.to_string(),
            "label": txn.label,
            "actor": txn.author.actor.to_string(),
            "entities": entities_of(txn.ops()),
            "pending": true,
        }),
        (None, Some((id, _))) if method == "txn.abort" => json!({
            "method": method,
            "txn": id.to_string(),
            "entities": before.open_entities,
            "pending": true,
            "aborted": true,
        }),
        _ => return,
    };
    engine.events.publish(Topic::pending, params);
}

/// Logs, and publishes as an edit event, a transaction the call committed, undid or redid.
fn publish_edit(engine: &mut Engine, method: &str, before: &Marker) {
    let Some(scene) = engine.scene.as_ref() else {
        return;
    };
    let (done, undone) = (scene.log.done(), scene.log.undone());
    let (txn, undo) = match (
        done.len().cmp(&before.done),
        undone.len().cmp(&before.undone),
    ) {
        (std::cmp::Ordering::Greater, _) => (done.last(), false),
        (std::cmp::Ordering::Less, std::cmp::Ordering::Greater) => (undone.last(), true),
        _ => return,
    };
    let Some(txn) = txn else { return };
    let params = edit_params(method, txn, undo);
    let (actor, label, ops) = (
        txn.author.actor.to_string(),
        txn.label.clone(),
        txn.ops.len(),
    );
    match (undo, method) {
        (true, _) => {
            tracing::info!(target: "concordia::edit", actor, label, ops, method, "edit undone")
        }
        (false, "txn.redo") => {
            tracing::info!(target: "concordia::edit", actor, label, ops, method, "edit redone")
        }
        (false, _) => {
            tracing::info!(target: "concordia::edit", actor, label, ops, method, "edit committed")
        }
    }
    if engine.events.wants(Topic::edit) {
        engine.events.publish(Topic::edit, params);
    }
}

fn edit_params(method: &str, txn: &Transaction, undo: bool) -> Value {
    let entities = entities_of(&txn.ops);
    json!({
        "method": method,
        "txn": txn.id.to_string(),
        "label": txn.label,
        "actor": txn.author.actor.to_string(),
        "entities": entities,
        "undo": undo,
    })
}

/// The distinct entities `ops` touch, in first-touched order.
fn entities_of(ops: &[concordia_command::AppliedOp]) -> Vec<String> {
    let mut entities: Vec<String> = Vec::new();
    for applied in ops {
        let id = match &applied.op {
            EditOp::Spawn { id, .. }
            | EditOp::Despawn { id, .. }
            | EditOp::SetComponent { id, .. }
            | EditOp::RemoveComponent { id, .. } => id.to_string(),
        };
        if !entities.contains(&id) {
            entities.push(id);
        }
    }
    entities
}

/// Announces log records the call wrote (no event when there are none).
fn publish_log(engine: &mut Engine, before: &Marker) {
    let next = engine.ring.next_seq();
    if next > before.log && engine.events.wants(Topic::log) {
        engine
            .events
            .publish(Topic::log, json!({"latest": next - 1}));
    }
}
