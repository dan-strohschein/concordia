//! Agent Studio on the server (ADR-0080): per-agent engine sessions, scope checks and task budgets.
//!
//! The PreToolUse hook stamps each engine call with a reserved `_studio` key (role, agent). This
//! gate strips it (and any model-supplied `actor`), re-checks the role's scope with the same
//! policy the hook uses, and runs the call in that agent's own engine session as
//! `agent:<role>#n`. Calls without the stamp are read-only.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use concordia_api::{ApiError, ApiErrorKind, EngineLink, MethodInfo};
use concordia_command::Actor;
use concordia_core::id::SessionId;
use concordia_studio::{Decision, Policy, Roster, Slots, ToolCatalog, source_hash};
use rmcp::model::JsonObject;
use serde_json::{Value, json};

/// The reserved key the hook adds to tool input.
const ENVELOPE: &str = "_studio";
/// Who budget escalations are attributed to.
const SYSTEM: &str = "system:studio";

/// One agent's engine session.
#[derive(Debug, Clone)]
struct Agent {
    session: SessionId,
    actor: Actor,
}

/// Studio enforcement for one `concordia mcp` server.
#[derive(Debug)]
pub(crate) struct StudioGate {
    root: PathBuf,
    studio: PathBuf,
    slots: Slots,
    policy: Mutex<Option<Policy>>,
    agents: Mutex<HashMap<String, Agent>>,
    budgets: Mutex<HashMap<(String, String), (u32, u32)>>,
}

/// The `_studio` stamp.
struct Stamp {
    role: Option<String>,
    agent: String,
}

impl StudioGate {
    /// The gate for the project at `root` with Studio files in `<root>/<studio>`.
    pub(crate) fn new(root: &Path, studio: &Path) -> Self {
        Self {
            root: root.to_owned(),
            studio: studio.to_owned(),
            slots: Slots::of_project(root),
            policy: Mutex::new(None),
            agents: Mutex::new(HashMap::new()),
            budgets: Mutex::new(HashMap::new()),
        }
    }

    /// Checks one call and returns the engine session to run it in; strips `_studio` and `actor`.
    pub(crate) async fn admit(
        &self,
        engine: &dyn EngineLink,
        base: SessionId,
        method: &MethodInfo,
        args: &mut JsonObject,
    ) -> Result<SessionId, ApiError> {
        let stamp = args.remove(ENVELOPE);
        args.remove("actor");
        let Some(stamp) = stamp.map(parse_stamp).transpose()? else {
            return match method.mutates {
                true => Err(not_permitted(
                    "Agent Studio is on, but this call has no Studio stamp, so it is read-only. \
                     Check that .claude/settings.json has the Studio hooks (`concordia studio sync`) \
                     and that `concordia` is on PATH for Claude Code.",
                )),
                false => Ok(base),
            };
        };
        let policy = self.policy(engine)?;
        let role = stamp.role.as_deref();
        if let Decision::Deny(reason) = policy.tool(role, &method.tool_name()) {
            return Err(not_permitted(&reason));
        }
        let Some(role) = role else {
            return Ok(base);
        };
        self.check_paths(engine, base, &policy, role, method, args)
            .await?;
        let agent = self.agent(engine, &policy, role, &stamp.agent).await?;
        self.charge(engine, base, &policy, &agent.actor, method.mutates)
            .await?;
        Ok(agent.session)
    }

    /// Forgets every agent's engine session (the engine they lived in has exited); slots are kept.
    pub(crate) fn forget_sessions(&self) {
        if let Ok(mut agents) = self.agents.lock() {
            agents.clear();
        }
    }

    /// The policy built from the engine's roster, rebuilt when `studio/` changes.
    fn policy(&self, engine: &dyn EngineLink) -> Result<Policy, ApiError> {
        let studio = self.root.join(&self.studio);
        let hash = source_hash(&studio).map_err(config)?;
        let mut cached = self.policy.lock().map_err(|_| poisoned())?;
        if let Some(policy) = cached.as_ref().filter(|p| p.source_hash == hash) {
            return Ok(policy.clone());
        }
        let catalog = ToolCatalog::from_methods(engine.methods());
        let roster = Roster::load(&studio, &catalog).map_err(config)?;
        let policy = Policy::build(&roster, &catalog, &self.studio, hash);
        *cached = Some(policy.clone());
        Ok(policy)
    }

    /// Scene, asset and material writes must land inside the role's paths.
    async fn check_paths(
        &self,
        engine: &dyn EngineLink,
        base: SessionId,
        policy: &Policy,
        role: &str,
        method: &MethodInfo,
        args: &JsonObject,
    ) -> Result<(), ApiError> {
        let given = args.get("path").and_then(Value::as_str).map(str::to_owned);
        let path = match (method.name, given) {
            ("scene.new" | "scene.save" | "asset.import" | "material.set", Some(path)) => path,
            ("scene.save", None) => match engine.call(base, "scene.info", json!({})).await {
                Ok(info) => info["path"].as_str().unwrap_or_default().to_owned(),
                Err(_) => return Ok(()),
            },
            _ => return Ok(()),
        };
        let decision = match method.name {
            "asset.import" => import_decision(policy, role, &path),
            _ => policy.write(Some(role), &path),
        };
        match decision {
            Decision::Deny(reason) => Err(not_permitted(&reason).at("/path")),
            _ => Ok(()),
        }
    }

    /// The agent's session, claiming an instance slot on first use.
    async fn agent(
        &self,
        engine: &dyn EngineLink,
        policy: &Policy,
        role: &str,
        key: &str,
    ) -> Result<Agent, ApiError> {
        if let Some(agent) = self.agents.lock().map_err(|_| poisoned())?.get(key) {
            return Ok(agent.clone());
        }
        let count = policy.roles.get(role).map_or(1, |r| r.count);
        let n = self
            .slots
            .claim(role, key, count, std::process::id())
            .map_err(|e| not_permitted(&e.to_string()))?;
        let actor: Actor =
            format!("agent:{role}#{n}")
                .parse()
                .map_err(|e: concordia_command::ActorError| {
                    ApiError::new(ApiErrorKind::ActorMalformed, e.to_string())
                })?;
        let session = engine.open_session().await?;
        engine
            .call(
                session,
                "session.open",
                json!({"client": "mcp-studio", "actor": actor.to_string()}),
            )
            .await?;
        let agent = Agent { session, actor };
        self.agents
            .lock()
            .map_err(|_| poisoned())?
            .insert(key.to_owned(), agent.clone());
        Ok(agent)
    }

    /// Counts the call against the agent's in-progress task; over budget blocks the task.
    async fn charge(
        &self,
        engine: &dyn EngineLink,
        base: SessionId,
        policy: &Policy,
        actor: &Actor,
        mutates: bool,
    ) -> Result<(), ApiError> {
        let limits = &policy.budgets;
        if limits.calls.is_none() && limits.mutations.is_none() {
            return Ok(());
        }
        let Some(task) = current_task(engine, base, actor).await else {
            return Ok(());
        };
        let (calls, mutations) = {
            let mut budgets = self.budgets.lock().map_err(|_| poisoned())?;
            let entry = budgets
                .entry((actor.to_string(), task.clone()))
                .or_default();
            entry.0 += 1;
            entry.1 += u32::from(mutates);
            *entry
        };
        let over = limits.calls.is_some_and(|max| calls > max)
            || limits.mutations.is_some_and(|max| mutations > max);
        if !over {
            return Ok(());
        }
        let block =
            json!({"id": task, "status": "blocked", "assignee": "human:local", "actor": SYSTEM});
        let _ = engine.call(base, "task.update", block).await;
        Err(not_permitted(&format!(
            "{actor} used up the engine budget for {task} (studio/team.toml [budgets]); {task} is now blocked for a human"
        )))
    }
}

/// The task `actor` is working on (in progress and assigned to it), if any.
async fn current_task(engine: &dyn EngineLink, base: SessionId, actor: &Actor) -> Option<String> {
    let params = json!({"status": "in_progress", "assignee": actor.to_string()});
    let listed = engine.call(base, "task.list", params).await.ok()?;
    listed["tasks"][0]["id"].as_str().map(str::to_owned)
}

/// Importing writes only `<path>.import.ron`: a role may import a file it may write, or one whose
/// sidecar it may write (the animator's clip events on a modeler's model, M2 2.4.5).
fn import_decision(policy: &Policy, role: &str, path: &str) -> Decision {
    let source = policy.write(Some(role), path);
    if !matches!(source, Decision::Deny(_)) {
        return source;
    }
    let sidecar = format!("{path}{}", concordia_assets::Sidecar::SUFFIX);
    match policy.write(Some(role), &sidecar) {
        Decision::Deny(_) => source,
        allowed => allowed,
    }
}

fn parse_stamp(value: Value) -> Result<Stamp, ApiError> {
    let agent = value["agent"].as_str().filter(|a| !a.is_empty());
    let role = value["role"].as_str().map(str::to_owned);
    match agent {
        Some(agent) => Ok(Stamp {
            role,
            agent: agent.to_owned(),
        }),
        None => Err(not_permitted(
            "the Studio stamp has no agent; it must come from the Studio hook",
        )),
    }
}

fn not_permitted(message: &str) -> ApiError {
    ApiError::new(ApiErrorKind::NotPermitted, message)
}

fn config(e: concordia_studio::StudioError) -> ApiError {
    ApiError::new(ApiErrorKind::SceneParse, e.to_string())
        .suggest(["fix the Studio file; `concordia studio roster` checks it".to_owned()])
}

fn poisoned() -> ApiError {
    ApiError::new(
        ApiErrorKind::Internal,
        "the Studio gate's state is poisoned",
    )
}
