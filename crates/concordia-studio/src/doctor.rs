//! Machine checks for Agent Studio: is Claude Code installed and logged in, is `concordia` on
//! PATH, are the generated files fresh? Shared by `concordia studio doctor`, `studio.doctor` and
//! the editor's launcher. A subscription login is all Studio needs; an API key is never required.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// What to do when Claude Code is missing.
pub const INSTALL_HINT: &str = "the Claude Code CLI (`claude`) is not on PATH; install it from https://code.claude.com and log in with `claude`";

/// Login methods that come from a Claude subscription.
pub const SUBSCRIPTION_METHODS: [&str; 2] = ["claude.ai", "oauth_token"];

/// What the checks may look at: the search path and whether an API key is set (never its value).
#[derive(Debug, Clone, Default)]
pub struct DoctorEnv {
    /// `PATH`.
    pub path: Option<OsString>,
    /// True if `ANTHROPIC_API_KEY` is set.
    pub api_key_set: bool,
}

impl DoctorEnv {
    /// This process's environment.
    pub fn from_process() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            api_key_set: std::env::var_os("ANTHROPIC_API_KEY").is_some(),
        }
    }

    /// `program` on the search path.
    pub fn find(&self, program: &str) -> Option<PathBuf> {
        std::env::split_paths(self.path.as_ref()?)
            .map(|dir| dir.join(program))
            .find(|path| path.is_file())
    }

    /// Runs `program args` from the search path; its stdout if it succeeded.
    fn output(&self, program: &str, args: &[&str]) -> Option<String> {
        let mut command = Command::new(self.find(program)?);
        command.args(args);
        if let Some(path) = &self.path {
            command.env("PATH", path);
        }
        let out = command.output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}

/// A check's outcome.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum CheckStatus {
    /// Fine.
    ok,
    /// Worth knowing; not a problem.
    note,
    /// Studio won't work until it is fixed.
    fail,
}

/// One check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Check {
    /// `claude`, `login`, `api_key`, `concordia` or `sync`.
    pub id: String,
    /// Outcome.
    pub status: CheckStatus,
    /// What was found and what to do.
    pub text: String,
}

/// The `claude` executable on the search path.
pub fn claude_path(env: &DoctorEnv) -> Option<PathBuf> {
    env.find("claude")
}

/// `claude --version`.
pub fn claude_version(env: &DoctorEnv) -> Option<String> {
    env.output("claude", &["--version"])
}

/// The `authMethod` of `claude auth status` (`claude.ai` for a subscription login).
pub fn auth_method(env: &DoctorEnv) -> Option<String> {
    let status = env.output("claude", &["auth", "status"])?;
    let json: serde_json::Value = serde_json::from_str(&status).ok()?;
    json["authMethod"].as_str().map(str::to_owned)
}

/// The checks that need no project: Claude Code, its login, an API key, `concordia` on PATH.
pub fn machine_checks(env: &DoctorEnv) -> Vec<Check> {
    [claude(env), login(env), api_key(env), concordia(env)]
        .into_iter()
        .flatten()
        .collect()
}

/// The freshness check from a `studio sync --check` outcome (`Ok(stale)`).
pub fn sync_check(stale: Result<bool, String>) -> Check {
    match stale {
        Ok(false) => check(
            "sync",
            CheckStatus::ok,
            "the Claude Code files are in sync with studio/",
        ),
        Ok(true) => check(
            "sync",
            CheckStatus::fail,
            "the Claude Code files are out of date; run `concordia studio sync`",
        ),
        Err(e) => check(
            "sync",
            CheckStatus::fail,
            format!("the Studio files have a problem: {e}"),
        ),
    }
}

fn check(id: &str, status: CheckStatus, text: impl Into<String>) -> Check {
    Check {
        id: id.to_owned(),
        status,
        text: text.into(),
    }
}

fn claude(env: &DoctorEnv) -> Option<Check> {
    Some(match claude_version(env) {
        Some(version) => check("claude", CheckStatus::ok, format!("Claude Code {version}")),
        None => check("claude", CheckStatus::fail, INSTALL_HINT),
    })
}

fn login(env: &DoctorEnv) -> Option<Check> {
    claude_path(env)?;
    let method = auth_method(env).unwrap_or_else(|| "unknown".into());
    Some(match method.as_str() {
        m if SUBSCRIPTION_METHODS.contains(&m) => check(
            "login",
            CheckStatus::ok,
            format!("logged in with a Claude subscription ({method}); no API key needed"),
        ),
        "none" => check(
            "login",
            CheckStatus::fail,
            "Claude Code is not logged in; run `claude` and log in with your subscription",
        ),
        other => check(
            "login",
            CheckStatus::note,
            format!(
                "Claude Code logs in with {other}; Agent Studio works with a plain subscription login and needs no API key"
            ),
        ),
    })
}

fn api_key(env: &DoctorEnv) -> Option<Check> {
    env.api_key_set.then(|| {
        check(
            "api_key",
            CheckStatus::note,
            "ANTHROPIC_API_KEY is set; Agent Studio does not use it",
        )
    })
}

fn concordia(env: &DoctorEnv) -> Option<Check> {
    Some(match env.find("concordia") {
        Some(path) => check(
            "concordia",
            CheckStatus::ok,
            format!(
                "`concordia` is on PATH ({}), so the Studio hooks can run",
                path.display()
            ),
        ),
        None => check(
            "concordia",
            CheckStatus::fail,
            "`concordia` is not on PATH, so Claude Code can't run the Studio hooks or the MCP server; install it (e.g. `cargo install --path crates/concordia-cli`)",
        ),
    })
}
