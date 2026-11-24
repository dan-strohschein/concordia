//! Helpers for the Studio checkpoint gates (`tests/gates/m3s_*.rs`): a fresh `concordia new`
//! project, and Claude Code hook inputs fed through the real `concordia studio hook` binary,
//! whose `updatedInput` then goes to a real `concordia mcp`, exactly as Claude Code would do.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::bins::concordia_bin;
use crate::mcp_client::{McpClient, ToolError};

/// The Claude Code session id every scripted hook input carries.
pub const SESSION: &str = "gate-session";

/// `concordia new game` in `dir` (Studio on, synced); returns the project root.
pub fn new_project(dir: &Path) -> PathBuf {
    let out = Command::new(concordia_bin())
        .args(["new", "game"])
        .current_dir(dir)
        .output()
        .expect("run concordia new");
    assert!(
        out.status.success(),
        "concordia new: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir.join("game")
}

/// `concordia mcp` on `root`, as `.mcp.json` starts it.
pub async fn mcp(root: &Path) -> McpClient {
    let project = root.to_str().expect("utf-8 project path");
    McpClient::spawn(&concordia_bin(), &["mcp", "--project", project])
        .await
        .expect("start concordia mcp")
}

/// What one hook run produced.
#[derive(Debug)]
pub struct HookRun {
    /// Exit code.
    pub code: Option<i32>,
    /// Parsed stdout (`Null` when empty).
    pub output: Value,
    /// Stderr.
    pub stderr: String,
}

/// Runs `concordia studio hook <event>` in `root` with `stdin` as its input.
pub fn hook_raw(root: &Path, event: &str, stdin: &str) -> HookRun {
    let mut child = Command::new(concordia_bin())
        .args(["studio", "hook", event])
        .current_dir(root)
        .env("CLAUDE_PROJECT_DIR", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run the hook");
    child
        .stdin
        .take()
        .expect("hook stdin")
        .write_all(stdin.as_bytes())
        .expect("write hook input");
    let out = child.wait_with_output().expect("hook output");
    let text = String::from_utf8_lossy(&out.stdout);
    HookRun {
        code: out.status.code(),
        output: match text.trim() {
            "" => Value::Null,
            body => serde_json::from_str(body).expect("hook stdout is JSON"),
        },
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Runs the hook with `input`, asserts exit 0, and returns its JSON output.
pub fn hook(root: &Path, event: &str, input: &Value) -> Value {
    let run = hook_raw(root, event, &input.to_string());
    assert_eq!(run.code, Some(0), "{event}: {}", run.stderr);
    run.output
}

/// Hook input for `role` running as Claude Code agent `agent` (no role: the main session).
pub fn input_as(event: &str, role: Option<&str>, agent: &str) -> Value {
    let mut input = json!({"session_id": SESSION, "transcript_path": "/tmp/t", "cwd": "/", "hook_event_name": event});
    if let Some(role) = role {
        input["agent_type"] = json!(role);
        input["agent_id"] = json!(agent);
    }
    input
}

/// Hook input for `role`'s first agent (no role: the main session, i.e. the director).
pub fn input(event: &str, role: Option<&str>) -> Value {
    input_as(event, role, &format!("agent-{}", role.unwrap_or("main")))
}

/// A hook's `additionalContext`.
pub fn context(output: &Value) -> String {
    output["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// A PreToolUse decision (`allow`, `deny`, or "" for none).
pub fn decision(output: &Value) -> &str {
    output["hookSpecificOutput"]["permissionDecision"]
        .as_str()
        .unwrap_or_default()
}

/// The PreToolUse hook's answer for `tool_name(tool_input)` from `role`'s agent `agent`.
pub fn pre_tool_use(
    root: &Path,
    role: Option<&str>,
    agent: &str,
    tool_name: &str,
    tool_input: Value,
) -> Value {
    let mut pre = input_as("PreToolUse", role, agent);
    pre["tool_name"] = json!(tool_name);
    pre["tool_input"] = tool_input;
    pre["tool_use_id"] = json!(format!("toolu-{tool_name}-{agent}"));
    hook(root, "pre-tool-use", &pre)
}

/// An engine tool call as `role`'s agent `agent` makes it: the hook must allow and stamp it, then
/// `concordia mcp` runs the stamped input.
pub async fn agent_try_as(
    root: &Path,
    mcp: &McpClient,
    role: Option<&str>,
    agent: &str,
    tool: &str,
    args: Value,
) -> Result<Value, ToolError> {
    let out = pre_tool_use(root, role, agent, &format!("mcp__concordia__{tool}"), args);
    assert_eq!(decision(&out), "allow", "{role:?} {tool}: {out}");
    mcp.call(tool, out["hookSpecificOutput"]["updatedInput"].clone())
        .await
}

/// [`agent_try_as`] for `role`'s first agent.
pub async fn agent_try(
    root: &Path,
    mcp: &McpClient,
    role: Option<&str>,
    tool: &str,
    args: Value,
) -> Result<Value, ToolError> {
    let agent = format!("agent-{}", role.unwrap_or("main"));
    agent_try_as(root, mcp, role, &agent, tool, args).await
}

/// [`agent_try`] that must succeed.
pub async fn agent_call(
    root: &Path,
    mcp: &McpClient,
    role: Option<&str>,
    tool: &str,
    args: Value,
) -> Value {
    agent_try(root, mcp, role, tool, args)
        .await
        .unwrap_or_else(|e| panic!("{role:?} {tool}: {}", e.text))
}

/// Runs `concordia studio <args>` in `root`.
pub fn studio(root: &Path, args: &[&str]) -> Output {
    Command::new(concordia_bin())
        .arg("studio")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run concordia studio")
}

/// A project file's text.
pub fn read(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

/// Generated Claude Code files: files directly in `dir` (agents) or one level down (skills), project-relative and sorted.
pub fn generated(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join(".claude/agents")).unwrap() {
        files.push(entry.unwrap().path());
    }
    for entry in std::fs::read_dir(root.join(".claude/skills")).unwrap() {
        files.push(entry.unwrap().path().join("SKILL.md"));
    }
    let mut files: Vec<PathBuf> = files
        .into_iter()
        .map(|p| p.strip_prefix(root).unwrap().to_owned())
        .collect();
    files.sort();
    files
}

/// Each generated agent and skill with its `name:` and `tools:` lines.
pub fn outline(root: &Path) -> String {
    generated(root)
        .iter()
        .map(|file| {
            let text = read(root, file.to_str().unwrap());
            let keep: Vec<&str> = text
                .lines()
                .filter(|l| l.starts_with("name: ") || l.starts_with("tools: "))
                .collect();
            format!("{}\n  {}", file.display(), keep.join("\n  "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The managed block of `CLAUDE.md`.
pub fn managed_block(root: &Path) -> String {
    let text = read(root, "CLAUDE.md");
    let start = text
        .find("<!-- concordia-studio:begin -->")
        .expect("begin marker");
    let end = text
        .find("<!-- concordia-studio:end -->")
        .expect("end marker");
    text[start..end].to_owned()
}

/// Each generated file's modification time.
pub fn mtimes(root: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut files = generated(root);
    files.extend(
        [".claude/settings.json", ".mcp.json", "CLAUDE.md"]
            .iter()
            .map(PathBuf::from),
    );
    files
        .into_iter()
        .map(|f| {
            let time = std::fs::metadata(root.join(&f))
                .unwrap()
                .modified()
                .unwrap();
            (f, time)
        })
        .collect()
}

/// A file edit as `role`'s first agent makes it in Claude Code: the PreToolUse hook decides; on
/// `allow` the file is written and the PostToolUse hook journals it. Returns the decision.
pub fn agent_edit(root: &Path, role: Option<&str>, path: &str, text: &str) -> String {
    let agent = format!("agent-{}", role.unwrap_or("main"));
    let file = root.join(path);
    let tool_input = json!({"file_path": file.display().to_string()});
    let pre = pre_tool_use(root, role, &agent, "Write", tool_input.clone());
    let verdict = decision(&pre).to_owned();
    if verdict == "allow" {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).expect("make the file's directory");
        }
        std::fs::write(&file, text).expect("write the file");
        let mut post = input_as("PostToolUse", role, &agent);
        post["tool_name"] = json!("Write");
        post["tool_input"] = tool_input;
        post["tool_use_id"] = json!(format!("toolu-Write-{agent}"));
        post["tool_response"] = json!({"success": true});
        hook(root, "post-tool-use", &post);
    }
    verdict
}
