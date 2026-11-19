//! `.claude/agents/<role>.md`: one Claude Code subagent per enabled role.

use serde::Serialize;

use super::owned::mark;
use crate::gate::{GateOn, GateParty};
use crate::role::{Effort, HarnessTool};
use crate::roster::{ResolvedRole, Roster};
use crate::task::TaskStatus;

/// The MCP server name in `.mcp.json`; tools appear as `mcp__concordia__<tool>`.
pub(crate) const SERVER: &str = "concordia";

/// Frontmatter in Claude Code's field names (only documented fields; never hooks, mcpServers or permissionMode).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Frontmatter<'a> {
    name: &'a str,
    description: &'a str,
    tools: String,
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<Effort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_turns: Option<u32>,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    skills: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'a str>,
}

/// The agent file for `role`, with the ownership marker.
pub(crate) fn agent_file(role: &ResolvedRole, roster: &Roster) -> String {
    let director = roster.director() == &role.role;
    let front = Frontmatter {
        name: role.role.as_str(),
        description: &role.description,
        tools: tools(role, roster, director).join(", "),
        model: &role.model,
        effort: role.effort,
        max_turns: role.max_turns.map(std::num::NonZeroU32::get),
        skills: &role.skills,
        color: role
            .color
            .as_ref()
            .map(crate::color::RoleColor::agent_color),
    };
    let yaml = serde_norway::to_string(&front).unwrap_or_default();
    let body = format!(
        "{}\n\n{}",
        role.body.trim_end(),
        footer(role, roster, director)
    );
    mark(
        &format!("---\n{yaml}---\n{body}"),
        &format!("studio/agents/{}.md", role.role),
    )
}

/// Harness tools, then `Agent(...)` for the director, then the role's engine tools.
fn tools(role: &ResolvedRole, roster: &Roster, director: bool) -> Vec<String> {
    let mut harness: Vec<HarnessTool> = role.harness_tools.clone();
    harness.sort();
    harness.dedup();
    let mut out: Vec<String> = harness
        .iter()
        .flat_map(|h| claude_names(*h).iter())
        .map(|name| (*name).to_owned())
        .collect();
    if director {
        let others: Vec<&str> = roster
            .roles()
            .iter()
            .filter(|r| r.role != role.role)
            .map(|r| r.role.as_str())
            .collect();
        out.push(format!("Agent({})", others.join(", ")));
    }
    out.extend(role.mcp_tools.iter().map(|t| format!("mcp__{SERVER}__{t}")));
    out
}

/// Claude Code tool names for a harness-neutral tool.
fn claude_names(tool: HarnessTool) -> &'static [&'static str] {
    match tool {
        HarnessTool::Read => &["Read"],
        HarnessTool::Search => &["Grep", "Glob"],
        HarnessTool::Edit => &["Edit"],
        HarnessTool::Write => &["Write"],
        HarnessTool::Bash => &["Bash"],
    }
}

/// Who may approve this role's tasks: every in_review → done gate plus the role's own rule.
pub(crate) fn reviewers(role: &ResolvedRole, roster: &Roster) -> Vec<GateParty> {
    let done = GateOn::TaskTransition {
        from: TaskStatus::in_review,
        to: TaskStatus::done,
    };
    let mut parties: Vec<GateParty> = roster
        .team
        .gates
        .iter()
        .filter(|g| g.on == done)
        .flat_map(|g| g.require_any.clone())
        .chain(role.review.clone())
        .collect();
    parties.dedup();
    parties
}

fn footer(role: &ResolvedRole, roster: &Roster, director: bool) -> String {
    let code = |items: &[String]| {
        items
            .iter()
            .map(|i| format!("`{i}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let reviewers: Vec<String> = reviewers(role, roster)
        .iter()
        .map(ToString::to_string)
        .collect();
    let writes = match role.write.is_empty() {
        true => {
            "- You change no project files; the task board changes only through `task_*` tools."
                .to_owned()
        }
        false => format!(
            "- Project paths you may change (through engine tools such as `scene_save`, or Edit/Write if you have them): {}.",
            code(&role.write)
        ),
    };
    let workflow = match director {
        true => {
            "Plan with `task_create` (clear acceptance criteria), then delegate each task to one role with the Agent tool, passing the task id. Route finished work to a reviewer and escalate blocked tasks to the human."
        }
        false => {
            "`task_claim` your task, do the work, record evidence with `task_comment`, then `task_update` it to `in_review`."
        }
    };
    format!(
        "## Your identity and limits\n\
- You act as `agent:{role}#<n>`; the engine assigns `#n` and records it with every change you make.\n\
- Change the game only through your `mcp__{SERVER}__*` tools.\n\
{writes} Never edit `studio/`, `.claude/`, `.mcp.json` or `concordia.toml`.\n\
- Reviews: {reviewers}.\n\
- Workflow: {workflow}\n",
        role = role.role,
        reviewers = match reviewers.is_empty() {
            true => "none required".to_owned(),
            false => format!("approval needs one of {}", code(&reviewers)),
        },
    )
}
