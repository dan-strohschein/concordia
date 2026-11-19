//! `.claude/settings.json` and `.mcp.json`: merge Studio's entries, keep everything else.

use serde_json::json;

use super::agents::SERVER;
use super::json::Json;
use crate::roster::Roster;

/// Hook events and the matcher groups Studio owns in each (event, matcher, hook subcommand).
const HOOKS: [(&str, Option<&str>, &str); 7] = [
    ("PreToolUse", Some("mcp__concordia__.*"), "pre-tool-use"),
    (
        "PreToolUse",
        Some("Edit|Write|NotebookEdit"),
        "pre-tool-use",
    ),
    ("PreToolUse", Some("Bash"), "pre-tool-use"),
    (
        "PostToolUse",
        Some("Edit|Write|NotebookEdit"),
        "post-tool-use",
    ),
    ("SubagentStart", None, "subagent-start"),
    ("SubagentStop", None, "subagent-stop"),
    ("SessionEnd", None, "session-end"),
];

/// Seconds Claude Code waits for a hook (its default is 600; ADR-0080 budgets 2 s).
const HOOK_TIMEOUT_S: u64 = 5;

/// `settings.json` with Studio's hooks, env and main agent merged in.
pub(crate) fn merge_settings(existing: Json, roster: &Roster) -> Json {
    let mut settings = existing;
    let hooks = settings.entry("hooks");
    for event in events() {
        let groups = match hooks.get(event) {
            Some(Json::Array(groups)) => groups.iter().filter_map(without_owned).collect(),
            _ => Vec::new(),
        };
        let ours = HOOKS
            .iter()
            .filter(|(e, _, _)| *e == event)
            .map(|(_, m, sub)| group(*m, sub));
        hooks.set(event, Json::Array(groups.into_iter().chain(ours).collect()));
    }
    let env = settings.entry("env");
    let concurrency = &roster.team.concurrency;
    env.set(
        "CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS",
        text(&concurrency.max_parallel_subagents.to_string()),
    );
    env.set(
        "CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH",
        text(&concurrency.max_spawn_depth.to_string()),
    );
    settings.set("agent", text(roster.director().as_str()));
    settings
}

/// `.mcp.json` with the `concordia` server set (other servers untouched).
pub(crate) fn merge_mcp(existing: Json) -> Json {
    let mut config = existing;
    let server =
        json!({"command": "concordia", "args": ["mcp", "--project", "${CLAUDE_PROJECT_DIR:-.}"]});
    config
        .entry("mcpServers")
        .set(SERVER, Json::from_value(server));
    config
}

fn events() -> Vec<&'static str> {
    let mut events: Vec<&str> = HOOKS.iter().map(|(e, _, _)| *e).collect();
    events.dedup();
    events
}

fn group(matcher: Option<&str>, subcommand: &str) -> Json {
    let handler = json!({
        "type": "command",
        "command": "concordia",
        "args": ["studio", "hook", subcommand],
        "timeout": HOOK_TIMEOUT_S,
    });
    let value = match matcher {
        Some(m) => json!({"matcher": m, "hooks": [handler]}),
        None => json!({"hooks": [handler]}),
    };
    Json::from_value(value)
}

/// A user's matcher group with Studio's handlers removed (none if nothing is left).
fn without_owned(group: &Json) -> Option<Json> {
    let Json::Object(fields) = group else {
        return Some(group.clone());
    };
    let mut kept = Vec::new();
    for (key, value) in fields {
        let value = match (key.as_str(), value) {
            ("hooks", Json::Array(handlers)) => {
                let mine: Vec<Json> = handlers.iter().filter(|h| !is_owned(h)).cloned().collect();
                if mine.is_empty() {
                    return None;
                }
                Json::Array(mine)
            }
            _ => value.clone(),
        };
        kept.push((key.clone(), value));
    }
    Some(Json::Object(kept))
}

/// Studio's handlers run `concordia studio hook ...`.
fn is_owned(handler: &Json) -> bool {
    let is = |key: &str, want: &str| matches!(handler.get(key), Some(Json::String(s)) if s == want);
    let args = match handler.get("args") {
        Some(Json::Array(args)) => args.clone(),
        _ => Vec::new(),
    };
    is("command", "concordia")
        && args.first() == Some(&text("studio"))
        && args.get(1) == Some(&text("hook"))
}

fn text(value: &str) -> Json {
    Json::String(value.to_owned())
}
