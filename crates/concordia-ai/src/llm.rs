//! The opt-in LLM seam (M4 4.12d, M9, D8). A `Plan` task submits a request to a provider and
//! polls it on later ticks, so nothing waits on the sim thread; the answer must name one of the
//! task's actions within its deadline, or the task fails and the tree's fallback runs. Every
//! applied answer is logged (`ExternalInputs`) so a replay needs no provider. The default build
//! has no provider, no model runtime and no network client: a shipped game brings its own.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A request's handle, for polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LlmTicket(pub u64);

/// Where a request stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmPoll {
    /// Not answered yet.
    Pending,
    /// The answer's text.
    Ready(String),
    /// It failed (the provider's reason).
    Failed(String),
}

/// Pick one action for a goal.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannerRequest {
    /// What the agent is trying to do.
    pub goal: String,
    /// What it knows (its blackboard, as JSON).
    pub observation: Value,
    /// The JSON Schema the answer must match (from [`action_schema`]).
    pub action_schema: Value,
}

/// Say something in character.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueRequest {
    /// Who speaks.
    pub speaker: String,
    /// What to respond to.
    pub prompt: String,
}

/// A request.
#[derive(Debug, Clone, PartialEq)]
pub enum LlmRequest {
    /// Planning.
    Planner(PlannerRequest),
    /// Dialogue.
    Dialogue(DialogueRequest),
}

/// A model the game developer chose and ships (never the developer's own subscription).
/// `submit` must return at once; the provider works on its own threads.
pub trait LlmProvider: Send + Sync {
    /// Starts a request.
    fn submit(&self, request: LlmRequest) -> LlmTicket;
    /// Checks on it.
    fn poll(&self, ticket: LlmTicket) -> LlmPoll;
}

/// The answer a planner gives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
#[reflect(name = "ai.PlannerReply")]
pub struct PlannerReply {
    /// The chosen action (one of the task's).
    pub action: String,
    /// Why (for logs).
    #[serde(default)]
    pub reason: Option<String>,
}

/// The JSON Schema of a [`PlannerReply`] whose `action` is one of `options` (from reflect).
pub fn action_schema(options: &[String]) -> Value {
    let mut schema = concordia_reflect::schema_for(PlannerReply::type_info());
    schema["properties"]["action"]["enum"] = Value::from(options.to_vec());
    schema
}

/// The action an answer names, if it is a valid reply naming one of `options`.
pub fn validate_reply(text: &str, options: &[String]) -> Result<String, String> {
    let reply: PlannerReply =
        serde_json::from_str(text).map_err(|e| format!("not a planner reply ({e}): {text:?}"))?;
    match options.contains(&reply.action) {
        true => Ok(reply.action),
        false => Err(format!("{:?} is not one of {options:?}", reply.action)),
    }
}

/// A scripted answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockReply {
    /// Polls before it is answered.
    pub polls: u32,
    /// The answer, or the failure.
    pub reply: Result<String, String>,
}

impl MockReply {
    /// Answers `text` after `polls` polls.
    pub fn ready(polls: u32, text: &str) -> Self {
        Self {
            polls,
            reply: Ok(text.to_owned()),
        }
    }

    /// Fails with `reason` after `polls` polls.
    pub fn failed(polls: u32, reason: &str) -> Self {
        Self {
            polls,
            reply: Err(reason.to_owned()),
        }
    }
}

/// A deterministic provider for tests and CI: scripted answers, in submission order.
#[derive(Debug, Default)]
pub struct MockLlmProvider {
    script: Mutex<VecDeque<MockReply>>,
    open: Mutex<BTreeMap<LlmTicket, MockReply>>,
    next: AtomicU64,
    requests: Mutex<Vec<LlmRequest>>,
}

impl MockLlmProvider {
    /// Answers requests with `replies`, in order (then fails every further request).
    pub fn new(replies: Vec<MockReply>) -> Self {
        Self {
            script: Mutex::new(replies.into()),
            ..Self::default()
        }
    }

    /// Every request submitted so far.
    pub fn requests(&self) -> Vec<LlmRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl LlmProvider for MockLlmProvider {
    fn submit(&self, request: LlmRequest) -> LlmTicket {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
        let ticket = LlmTicket(self.next.fetch_add(1, Ordering::Relaxed));
        let reply = self
            .script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(|| MockReply::failed(0, "no scripted reply"));
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(ticket, reply);
        ticket
    }

    fn poll(&self, ticket: LlmTicket) -> LlmPoll {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(reply) = open.get_mut(&ticket) else {
            return LlmPoll::Failed("unknown ticket".into());
        };
        if reply.polls > 0 {
            reply.polls -= 1;
            return LlmPoll::Pending;
        }
        match open.remove(&ticket).map(|r| r.reply) {
            Some(Ok(text)) => LlmPoll::Ready(text),
            Some(Err(reason)) => LlmPoll::Failed(reason),
            None => LlmPoll::Failed("unknown ticket".into()),
        }
    }
}

/// A plan waiting for its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pending {
    /// The provider's ticket (none when playing a recording back).
    pub(crate) ticket: Option<LlmTicket>,
    /// The last tick it may be answered on.
    pub(crate) deadline: u64,
}

/// The world's LLM provider (none by default) and the plans waiting on it.
#[derive(Clone, Default)]
pub struct LlmService {
    pub(crate) provider: Option<Arc<dyn LlmProvider>>,
    pub(crate) pending: Arc<Mutex<BTreeMap<(EntityId, u32), Pending>>>,
}

impl std::fmt::Debug for LlmService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmService")
            .field("provider", &self.provider.is_some())
            .finish()
    }
}

impl LlmService {
    /// With `provider`.
    pub fn with_provider(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider: Some(provider),
            pending: Arc::default(),
        }
    }

    pub(crate) fn pending(&self) -> std::sync::MutexGuard<'_, BTreeMap<(EntityId, u32), Pending>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
