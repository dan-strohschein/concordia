//! `concordia studio doctor`: is this machine ready to run Agent Studio in Claude Code?
//!
//! A Claude subscription login is all Studio needs; an API key is reported, never required.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Failure;

/// One check's result.
enum Check {
    Ok(String),
    Note(String),
    Fail(String),
}

pub(crate) fn run(project: &Path) -> Result<(), Failure> {
    let checks = [
        claude(),
        login(),
        api_key(),
        concordia_on_path(),
        in_sync(project),
    ];
    let mut failed = 0;
    for check in checks.into_iter().flatten() {
        let (tag, text) = match check {
            Check::Ok(text) => ("ok  ", text),
            Check::Note(text) => ("note", text),
            Check::Fail(text) => {
                failed += 1;
                ("FAIL", text)
            }
        };
        println!("{tag}  {text}");
    }
    match failed {
        0 => Ok(()),
        n => Err(Failure {
            code: 1,
            message: format!("{n} check(s) failed"),
        }),
    }
}

fn claude() -> Option<Check> {
    Some(match output("claude", &["--version"]) {
        Some(version) => Check::Ok(format!("Claude Code {}", version.trim())),
        None => Check::Fail(
            "the Claude Code CLI (`claude`) is not on PATH; install it from https://code.claude.com and log in with `claude`".into(),
        ),
    })
}

fn login() -> Option<Check> {
    let status = output("claude", &["auth", "status"])?;
    let method = serde_json::from_str::<serde_json::Value>(&status)
        .ok()
        .and_then(|v| v["authMethod"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into());
    Some(match method.as_str() {
        "claude.ai" | "oauth_token" => Check::Ok(format!(
            "logged in with a Claude subscription ({method}); no API key needed"
        )),
        "none" => Check::Fail(
            "Claude Code is not logged in; run `claude` and log in with your subscription".into(),
        ),
        other => Check::Note(format!(
            "Claude Code logs in with {other}; Agent Studio works with a plain subscription login and needs no API key"
        )),
    })
}

fn api_key() -> Option<Check> {
    std::env::var_os("ANTHROPIC_API_KEY")
        .map(|_| Check::Note("ANTHROPIC_API_KEY is set; Agent Studio does not use it".into()))
}

fn concordia_on_path() -> Option<Check> {
    Some(match on_path("concordia") {
        Some(path) => Check::Ok(format!("`concordia` is on PATH ({}), so the Studio hooks can run", path.display())),
        None => Check::Fail(
            "`concordia` is not on PATH, so Claude Code can't run the Studio hooks or the MCP server; install it (e.g. `cargo install --path crates/concordia-cli`)".into(),
        ),
    })
}

fn in_sync(project: &Path) -> Option<Check> {
    let manifest = concordia_app::Project::load(project).ok()?.manifest;
    manifest.studio.as_ref()?;
    let mut engine = super::project_engine(project).ok()?;
    let session = engine.open_session();
    let params = serde_json::json!({"check": true, "actor": "human:local"});
    Some(match engine.call(session, "studio.sync", params) {
        Ok(report) if report["stale"] == false => {
            Check::Ok("the Claude Code files are in sync with studio/".into())
        }
        Ok(_) => {
            Check::Fail("the Claude Code files are out of date; run `concordia studio sync`".into())
        }
        Err(e) => Check::Fail(format!("the Studio files have a problem: {}", e.message)),
    })
}

/// Runs `program args` and returns its stdout if it succeeded.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn on_path(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|path| path.is_file())
}
