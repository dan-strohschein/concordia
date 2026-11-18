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

    fn wants(&self, topic: Topic) -> bool {
        self.topics.values().any(|t| t.contains(&topic))
    }

    /// Sends `events.event` with `params` (plus `topic`) to every session subscribed to `topic`.
    fn publish(&mut self, topic: Topic, mut params: Value) {
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
    done: usize,
    undone: usize,
}

impl Marker {
    pub(crate) fn of(engine: &Engine) -> Self {
        let (done, undone) = engine
            .scene
            .as_ref()
            .map_or((0, 0), |s| (s.log.done().len(), s.log.undone().len()));
        Self { done, undone }
    }
}

/// Publishes the events a successful `method` call produced.
pub(crate) fn publish_after(engine: &mut Engine, method: &str, before: &Marker, result: &Value) {
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
        _ if engine.events.wants(Topic::edit) => publish_edit(engine, method, before),
        _ => {}
    }
}

/// An edit event if the call committed, undid or redid a transaction.
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
    engine.events.publish(Topic::edit, params);
}

fn edit_params(method: &str, txn: &Transaction, undo: bool) -> Value {
    let mut entities: Vec<String> = Vec::new();
    for applied in &txn.ops {
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
    json!({
        "method": method,
        "txn": txn.id.to_string(),
        "label": txn.label,
        "actor": txn.author.actor.to_string(),
        "entities": entities,
        "undo": undo,
    })
}
