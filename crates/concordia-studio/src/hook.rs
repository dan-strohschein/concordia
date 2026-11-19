//! `concordia studio hook <event>`: Claude Code hooks that stamp identity and enforce scopes (ADR-0080).
//!
//! The hook reads `.claude/concordia/policy.json` (written by sync) and never starts an engine.
//! PreToolUse fails closed: unreadable input exits 2, and a missing or stale policy denies. The
//! other events fail open with a warning, since they only record or inform.

use std::path::Path;

use concordia_command::Actor;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::journal::{JournalEvent, StudioJournal};
use crate::policy::{Decision, Policy, project_relative};
use crate::slots::Slots;
use crate::task::{Task, TaskStatus};

/// The hook events Studio handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    /// Before a tool runs: stamp engine calls, check scopes.
    PreToolUse,
    /// After a tool ran: record file edits.
    PostToolUse,
    /// A subagent started: tell it who it is.
    SubagentStart,
    /// A subagent stopped: release its instance slot.
    SubagentStop,
    /// The session ended: release every slot it held (the main session gets no SubagentStop).
    SessionEnd,
}

impl HookEvent {
    /// Parses the CLI name (`pre-tool-use`, ...).
    pub fn from_cli(name: &str) -> Option<Self> {
        match name {
            "pre-tool-use" => Some(Self::PreToolUse),
            "post-tool-use" => Some(Self::PostToolUse),
            "subagent-start" => Some(Self::SubagentStart),
            "subagent-stop" => Some(Self::SubagentStop),
            "session-end" => Some(Self::SessionEnd),
            _ => None,
        }
    }
}

/// What the hook process prints and returns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookOutcome {
    /// JSON for Claude Code (empty: no decision).
    pub stdout: String,
    /// Diagnostics.
    pub stderr: String,
    /// 0 normally; 2 blocks a PreToolUse call.
    pub exit: i32,
}

/// The engine MCP server's tool prefix.
const ENGINE_TOOLS: &str = "mcp__concordia__";
const FILE_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// The documented hook input fields Studio uses (others are ignored).
#[derive(Debug, Deserialize)]
struct Input {
    session_id: String,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    agent_type: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<Value>,
    #[serde(default)]
    tool_use_id: Option<String>,
}

/// Who is calling: the role (none: an agent outside the team) and the agent key.
struct Caller {
    role: Option<String>,
    agent: String,
}

/// Handles one hook call for the project at `root`.
pub fn run(event: HookEvent, input: &str, root: &Path) -> HookOutcome {
    let input: Input = match serde_json::from_str(input) {
        Ok(input) => input,
        Err(e) => {
            return failure(
                event,
                format!("Agent Studio could not read the hook input: {e}"),
            );
        }
    };
    match event {
        HookEvent::PreToolUse => pre_tool_use(&input, root),
        HookEvent::PostToolUse => post_tool_use(&input, root).unwrap_or_else(|e| failure(event, e)),
        HookEvent::SubagentStart => subagent_start(&input, root),
        HookEvent::SubagentStop => {
            subagent_stop(&input, root).unwrap_or_else(|e| failure(event, e))
        }
        HookEvent::SessionEnd => Slots::of_project(root)
            .release_session(&input.session_id)
            .map(|()| HookOutcome::default())
            .unwrap_or_else(|e| failure(event, e.to_string())),
    }
}

fn pre_tool_use(input: &Input, root: &Path) -> HookOutcome {
    let tool = input.tool_name.as_deref().unwrap_or_default();
    let engine_tool = tool.strip_prefix(ENGINE_TOOLS);
    if engine_tool.is_none() && !FILE_TOOLS.contains(&tool) && tool != "Bash" {
        return HookOutcome::default();
    }
    let policy = match current_policy(root) {
        Ok(policy) => policy,
        Err(reason) => return pre_decision("deny", &reason, None),
    };
    let caller = caller(input, &policy);
    let role = caller.role.as_deref();
    let tool_input = input.tool_input.clone().unwrap_or_else(|| json!({}));
    let decision = match engine_tool {
        Some(name) => policy.tool(role, name),
        None if tool == "Bash" => {
            policy.bash(role, tool_input["command"].as_str().unwrap_or_default())
        }
        None => file_decision(
            &policy,
            role,
            &tool_input,
            root,
            input.tool_use_id.as_deref(),
        ),
    };
    match decision {
        Decision::Pass => HookOutcome::default(),
        Decision::Deny(reason) => pre_decision("deny", &format!("Agent Studio: {reason}"), None),
        Decision::Allow => {
            let who = role.map_or_else(|| "this agent".to_owned(), |r| format!("role {r}"));
            let stamped =
                engine_tool.map(|_| stamp(tool_input, &caller, input.tool_use_id.as_deref()));
            pre_decision(
                "allow",
                &format!("Agent Studio: {who} may use {tool}"),
                stamped,
            )
        }
    }
}

/// The policy, or why the hook can't trust it.
fn current_policy(root: &Path) -> Result<Policy, String> {
    let policy = Policy::load(root).map_err(|e| {
        format!("Agent Studio has no usable policy ({e}); run `concordia studio sync`")
    })?;
    match policy.is_stale(root) {
        true => Err(
            "Agent Studio: studio/ changed since the last sync; run `concordia studio sync`".into(),
        ),
        false => Ok(policy),
    }
}

/// The caller's role: the subagent's type, the main session's role, or none (outside the team).
fn caller(input: &Input, policy: &Policy) -> Caller {
    let role = match input.agent_type.as_deref() {
        None | Some("") => Some(policy.main_session_role.clone()),
        Some(kind) if policy.roles.contains_key(kind) => Some(kind.to_owned()),
        Some(_) => None,
    };
    let agent = format!(
        "{}/{}",
        input.session_id,
        input.agent_id.as_deref().unwrap_or("main")
    );
    Caller { role, agent }
}

/// The tool input with the reserved `_studio` key that `concordia mcp` reads and strips.
fn stamp(mut tool_input: Value, caller: &Caller, tool_use_id: Option<&str>) -> Value {
    if !tool_input.is_object() {
        tool_input = json!({});
    }
    tool_input["_studio"] =
        json!({"role": caller.role, "agent": caller.agent, "tool_use_id": tool_use_id});
    tool_input
}

fn file_decision(
    policy: &Policy,
    role: Option<&str>,
    tool_input: &Value,
    root: &Path,
    tool_use_id: Option<&str>,
) -> Decision {
    let Some(path) = file_path(tool_input) else {
        return Decision::Deny("the edit names no file".into());
    };
    let Some(relative) = project_relative(root, Path::new(path)) else {
        return Decision::Deny(format!("{path} is outside the project"));
    };
    let decision = policy.write(role, &relative);
    if decision == Decision::Allow {
        remember_before(root, tool_use_id, &relative);
    }
    decision
}

fn file_path(tool_input: &Value) -> Option<&str> {
    tool_input["file_path"]
        .as_str()
        .or_else(|| tool_input["notebook_path"].as_str())
}

/// Saves the file's hash before an allowed edit, for PostToolUse's audit entry.
fn remember_before(root: &Path, tool_use_id: Option<&str>, relative: &str) {
    let Some(id) = tool_use_id.filter(|id| {
        id.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }) else {
        return;
    };
    let dir = root.join(".concordia/studio/pending");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(
        dir.join(id),
        file_hash(&root.join(relative)).unwrap_or_default(),
    );
}

fn post_tool_use(input: &Input, root: &Path) -> Result<HookOutcome, String> {
    let tool = input.tool_name.as_deref().unwrap_or_default();
    let tool_input = input.tool_input.clone().unwrap_or_default();
    let (true, Some(path)) = (FILE_TOOLS.contains(&tool), file_path(&tool_input)) else {
        return Ok(HookOutcome::default());
    };
    let Some(relative) = project_relative(root, Path::new(path)) else {
        return Ok(HookOutcome::default());
    };
    let before = take_before(root, input.tool_use_id.as_deref());
    let after = file_hash(&root.join(&relative));
    let actor = audit_actor(input, root)?;
    let event = JournalEvent::FileEdit {
        path: relative,
        tool: tool.to_owned(),
        before,
        after,
    };
    StudioJournal::of_project(root)
        .append(&actor, event)
        .map_err(|e| e.to_string())?;
    Ok(HookOutcome::default())
}

fn take_before(root: &Path, tool_use_id: Option<&str>) -> Option<String> {
    let path = root.join(".concordia/studio/pending").join(tool_use_id?);
    let hash = std::fs::read_to_string(&path).ok();
    let _ = std::fs::remove_file(path);
    hash.filter(|h| !h.is_empty())
}

/// `agent:<role>#n` when the engine assigned a slot, else `agent:<role>`.
fn audit_actor(input: &Input, root: &Path) -> Result<Actor, String> {
    let policy = Policy::load(root).map_err(|e| e.to_string())?;
    let caller = caller(input, &policy);
    let slot = Slots::of_project(root).lookup(&caller.agent).ok().flatten();
    let name = match (&slot, &caller.role, input.agent_type.as_deref()) {
        (Some((role, n)), _, _) => format!("agent:{role}#{n}"),
        (None, Some(role), _) => format!("agent:{role}"),
        (None, None, kind) => format!("agent:{}", actor_name(kind.unwrap_or("unknown"))),
    };
    name.parse()
        .map_err(|e: concordia_command::ActorError| e.to_string())
}

/// A valid actor name from an arbitrary agent type (`Explore` → `explore`).
fn actor_name(kind: &str) -> String {
    let name: String = kind
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    match name.chars().next() {
        Some(c) if c.is_ascii_alphanumeric() => name.chars().take(64).collect(),
        _ => "unknown".into(),
    }
}

fn subagent_start(input: &Input, root: &Path) -> HookOutcome {
    let context = match Policy::load(root) {
        Ok(policy) => start_context(&policy, &caller(input, &policy), root),
        Err(_) => return HookOutcome::default(),
    };
    let out = json!({"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": context}});
    HookOutcome {
        stdout: out.to_string(),
        ..HookOutcome::default()
    }
}

fn start_context(policy: &Policy, caller: &Caller, root: &Path) -> String {
    let Some(role) = caller.role.as_deref() else {
        return "Agent Studio: you are outside the Studio team, so your engine access is read-only.".into();
    };
    let tasks: Vec<String> = open_tasks(&root.join(&policy.studio).join("tasks"), role)
        .iter()
        .map(|t| format!("- {} {} ({})", t.meta.id, t.meta.title, t.meta.status))
        .collect();
    let tasks = match tasks.is_empty() {
        true => "No open tasks for your role yet.".to_owned(),
        false => format!("Open tasks for your role:\n{}", tasks.join("\n")),
    };
    format!(
        "Agent Studio: you act as agent:{role} (the engine numbers your instance on your first engine call). Change the game only through your mcp__concordia__* tools; every change is recorded under your actor.\n{tasks}"
    )
}

/// The role's tasks that are not done, by id.
fn open_tasks(dir: &Path, role: &str) -> Vec<Task> {
    let mut tasks: Vec<Task> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".task.md"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|text| Task::parse(&text).ok())
        .filter(|t| t.meta.role == role && t.meta.status != TaskStatus::done)
        .collect();
    tasks.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
    tasks
}

fn subagent_stop(input: &Input, root: &Path) -> Result<HookOutcome, String> {
    let agent = format!(
        "{}/{}",
        input.session_id,
        input.agent_id.as_deref().unwrap_or("main")
    );
    Slots::of_project(root)
        .release(&agent)
        .map_err(|e| e.to_string())?;
    Ok(HookOutcome::default())
}

fn pre_decision(decision: &str, reason: &str, updated: Option<Value>) -> HookOutcome {
    let mut specific = json!({
        "hookEventName": "PreToolUse",
        "permissionDecision": decision,
        "permissionDecisionReason": reason,
    });
    if let Some(updated) = updated {
        specific["updatedInput"] = updated;
    }
    HookOutcome {
        stdout: json!({"hookSpecificOutput": specific}).to_string(),
        ..HookOutcome::default()
    }
}

/// PreToolUse fails closed (exit 2 blocks the call); other events warn and carry on.
fn failure(event: HookEvent, message: String) -> HookOutcome {
    HookOutcome {
        stdout: String::new(),
        stderr: message,
        exit: match event {
            HookEvent::PreToolUse => 2,
            _ => 0,
        },
    }
}

fn file_hash(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|bytes| blake3::hash(&bytes).to_hex()[..16].to_owned())
}
