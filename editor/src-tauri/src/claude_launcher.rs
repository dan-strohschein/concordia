//! The Claude Code launcher (M3 phase 3.8, ADR-0083): finds the user's own `claude`, shows how it
//! is logged in, and starts the Studio Director in the project, either interactively in a terminal
//! or headless with its stream shown in the editor.
//!
//! Children get the user's environment unchanged. The launcher never reads, stores or logs a
//! credential: it looks only at `PATH`, checks whether an API key *is set*, and its log
//! (`.concordia/launcher/launcher.log`) replaces every environment value with `<redacted:NAME>`.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};

use concordia_studio::doctor::{self, DoctorEnv};
use serde::Serialize;
use serde_json::Value;

/// The launcher's files, project-relative (inside the git-ignored `.concordia/`).
const DIR: &str = ".concordia/launcher";

/// Environment values shorter than this are left in logs (redacting "1" would garble every line).
const MIN_REDACTED_LEN: usize = 6;

/// Longest text kept from one stream line.
const SUMMARY_LEN: usize = 200;

/// Why a launch failed.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    /// Bad input.
    #[error("{0}")]
    Invalid(String),
    /// Claude Code is not installed.
    #[error("{0}")]
    NotInstalled(String),
    /// A file or process operation failed.
    #[error("{0}")]
    Io(String),
}

/// What the launcher found about Claude Code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Detection {
    /// The `claude` executable.
    pub claude: Option<PathBuf>,
    /// `claude --version`.
    pub version: Option<String>,
    /// `authMethod` from `claude auth status` (display only).
    pub auth_method: Option<String>,
    /// True for a subscription login (all Studio needs).
    pub subscription: bool,
    /// How to install Claude Code, when it is missing.
    pub install_hint: Option<String>,
}

/// One summarized stream-json line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StreamEvent {
    /// The subagent's `parent_tool_use_id`; `None` for the Director.
    pub group: Option<String>,
    /// The line's `type` (`system`, `assistant`, `user`, `result`).
    pub kind: String,
    /// A one-line summary.
    pub text: String,
}

/// The running headless child, so another thread can stop it.
#[derive(Debug, Clone, Default)]
pub struct RunSlot(Arc<Mutex<Option<Child>>>);

impl RunSlot {
    /// Stops the run, if one is going; true if there was one.
    pub fn stop(&self) -> bool {
        let Ok(mut slot) = self.0.lock() else {
            return false;
        };
        slot.as_mut().is_some_and(|child| child.kill().is_ok())
    }

    /// True while a run is going.
    pub fn busy(&self) -> bool {
        self.0.lock().is_ok_and(|slot| slot.is_some())
    }
}

/// Starts Claude Code for a project.
#[derive(Debug, Clone)]
pub struct Launcher {
    project: PathBuf,
    env: Vec<(OsString, OsString)>,
    opener: PathBuf,
}

impl Launcher {
    /// A launcher for `project` whose children get `env` (normally this process's environment).
    pub fn new(project: &Path, env: Vec<(OsString, OsString)>) -> Self {
        Self {
            project: project.to_owned(),
            env,
            opener: PathBuf::from("/usr/bin/open"),
        }
    }

    /// Uses `opener` instead of `/usr/bin/open` (tests).
    #[must_use]
    pub fn with_opener(mut self, opener: PathBuf) -> Self {
        self.opener = opener;
        self
    }

    /// What the doctor checks may see: the search path, and whether an API key is set.
    fn doctor_env(&self) -> DoctorEnv {
        DoctorEnv {
            path: self.var("PATH"),
            api_key_set: self.var("ANTHROPIC_API_KEY").is_some(),
        }
    }

    fn var(&self, name: &str) -> Option<OsString> {
        self.env
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }

    /// Finds `claude`, its version and its login method.
    pub fn detect(&self) -> Detection {
        let env = self.doctor_env();
        let claude = doctor::claude_path(&env);
        if claude.is_none() {
            return Detection {
                claude,
                version: None,
                auth_method: None,
                subscription: false,
                install_hint: Some(doctor::INSTALL_HINT.to_owned()),
            };
        }
        let auth_method = doctor::auth_method(&env);
        Detection {
            claude,
            version: doctor::claude_version(&env),
            subscription: auth_method
                .as_deref()
                .is_some_and(|m| doctor::SUBSCRIPTION_METHODS.contains(&m)),
            auth_method,
            install_hint: None,
        }
    }

    /// Opens the terminal `app` on a command file that runs `claude --agent director` in the
    /// project; returns the command file.
    pub fn open_terminal(&self, app: &str) -> Result<PathBuf, LaunchError> {
        let app = app.trim();
        if app.is_empty() || app.chars().any(char::is_control) {
            return Err(LaunchError::Invalid(
                "name the terminal app to open, e.g. Terminal or iTerm".into(),
            ));
        }
        let claude = doctor::claude_path(&self.doctor_env())
            .map_or_else(|| "claude".to_owned(), |p| quote(&p.to_string_lossy()));
        let file = self.dir()?.join("director.command");
        write_executable(&file, &command_file(&self.project, &claude))?;
        self.log(&format!("open -a {app} {}", file.display()));
        let status = Command::new(&self.opener)
            .args(["-a", app])
            .arg(&file)
            .status()
            .map_err(|e| LaunchError::Io(format!("could not run open: {e}")))?;
        match status.success() {
            true => Ok(file),
            false => Err(LaunchError::Io(format!(
                "open could not start {app} ({status}); is it installed?"
            ))),
        }
    }

    /// Runs `claude -p` as the Director and passes each summarized line to `on_event`.
    pub fn run_headless(
        &self,
        prompt: &str,
        max_budget_usd: Option<f64>,
        on_event: impl FnMut(StreamEvent),
    ) -> Result<ExitStatus, LaunchError> {
        self.run_headless_in(&RunSlot::default(), prompt, max_budget_usd, on_event)
    }

    /// [`Launcher::run_headless`], holding the child in `slot` so it can be stopped.
    pub fn run_headless_in(
        &self,
        slot: &RunSlot,
        prompt: &str,
        max_budget_usd: Option<f64>,
        mut on_event: impl FnMut(StreamEvent),
    ) -> Result<ExitStatus, LaunchError> {
        let argv = headless_argv(prompt, max_budget_usd)?;
        let claude = doctor::claude_path(&self.doctor_env())
            .ok_or_else(|| LaunchError::NotInstalled(doctor::INSTALL_HINT.into()))?;
        self.dir()?;
        let mut child = Command::new(&claude)
            .args(&argv)
            .current_dir(&self.project)
            .env_clear()
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| LaunchError::Io(format!("could not start claude: {e}")))?;
        self.log(&format!(
            "started claude -p --agent director (pid {})",
            child.id()
        ));
        let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
        hold(slot, Some(child))?;
        let errors = stderr.map(|err| {
            let me = self.clone();
            std::thread::spawn(move || {
                BufReader::new(err)
                    .lines()
                    .map_while(Result::ok)
                    .for_each(|line| me.log(&format!("stderr: {line}")));
            })
        });
        for line in stdout
            .into_iter()
            .flat_map(|out| BufReader::new(out).lines().map_while(Result::ok))
        {
            match parse_stream_line(&line) {
                Some(event) => on_event(event),
                None => self.log(&format!("stdout: {line}")),
            }
        }
        if let Some(thread) = errors {
            let _ = thread.join();
        }
        let status = finish(slot)?;
        self.log(&format!("claude exited: {status}"));
        Ok(status)
    }

    /// `.concordia/launcher/`, created if needed.
    fn dir(&self) -> Result<PathBuf, LaunchError> {
        let dir = self.project.join(DIR);
        std::fs::create_dir_all(&dir)
            .map_err(|e| LaunchError::Io(format!("{}: {e}", dir.display())))?;
        Ok(dir)
    }

    /// Appends a redacted line to the launcher log (best effort).
    fn log(&self, message: &str) {
        let path = self.project.join(DIR).join("launcher.log");
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let line = format!("{ms} {}\n", redact(message, &self.env));
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| f.write_all(line.as_bytes()));
    }
}

fn hold(slot: &RunSlot, child: Option<Child>) -> Result<(), LaunchError> {
    let mut held = slot
        .0
        .lock()
        .map_err(|_| LaunchError::Io("run slot poisoned".into()))?;
    *held = child;
    Ok(())
}

/// Waits for the held child and empties the slot.
fn finish(slot: &RunSlot) -> Result<ExitStatus, LaunchError> {
    let child = slot
        .0
        .lock()
        .map_err(|_| LaunchError::Io("run slot poisoned".into()))?
        .take();
    let mut child = child.ok_or_else(|| LaunchError::Io("the run was lost".into()))?;
    child
        .wait()
        .map_err(|e| LaunchError::Io(format!("waiting for claude: {e}")))
}

/// The interactive session's command file: no environment, just the project and the Director.
fn command_file(project: &Path, claude: &str) -> String {
    format!(
        "#!/bin/sh\n# Opened by the Concordia editor: Claude Code as the Studio director.\ncd {} || exit 1\nexec {claude} --agent director\n",
        quote(&project.to_string_lossy())
    )
}

/// `s` in single quotes for `sh`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn write_executable(path: &Path, text: &str) -> Result<(), LaunchError> {
    let io = |e: std::io::Error| LaunchError::Io(format!("{}: {e}", path.display()));
    std::fs::write(path, text).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(io)?;
    }
    Ok(())
}

/// `claude` arguments for a headless Director run (never `--bare`, which skips the subscription
/// login and the project's hooks).
pub fn headless_argv(
    prompt: &str,
    max_budget_usd: Option<f64>,
) -> Result<Vec<String>, LaunchError> {
    if prompt.trim().is_empty() {
        return Err(LaunchError::Invalid("tell the Director what to do".into()));
    }
    let mut argv: Vec<String> = [
        "-p",
        prompt,
        "--agent",
        "director",
        "--output-format",
        "stream-json",
        "--verbose",
    ]
    .map(str::to_owned)
    .into();
    if let Some(budget) = max_budget_usd {
        if !budget.is_finite() || budget <= 0.0 {
            return Err(LaunchError::Invalid(format!(
                "headless_max_budget_usd must be a positive number, not {budget}"
            )));
        }
        argv.extend(["--max-budget-usd".to_owned(), budget.to_string()]);
    }
    Ok(argv)
}

/// `line` with every environment value (of at least 6 characters) replaced by `<redacted:NAME>`.
pub fn redact(line: &str, env: &[(OsString, OsString)]) -> String {
    let mut pairs: Vec<(String, String)> = env
        .iter()
        .filter_map(|(k, v)| Some((k.to_str()?.to_owned(), v.to_str()?.to_owned())))
        .filter(|(_, v)| v.len() >= MIN_REDACTED_LEN)
        .collect();
    pairs.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    pairs.iter().fold(line.to_owned(), |text, (name, value)| {
        text.replace(value, &format!("<redacted:{name}>"))
    })
}

/// Summarizes one stream-json line; `None` if it isn't one.
pub fn parse_stream_line(line: &str) -> Option<StreamEvent> {
    let json: Value = serde_json::from_str(line).ok()?;
    let kind = json["type"].as_str()?.to_owned();
    let text = match kind.as_str() {
        "assistant" | "user" => content_summary(&json["message"]["content"]),
        "result" => result_summary(&json),
        _ => json["subtype"].as_str().unwrap_or(&kind).to_owned(),
    };
    Some(StreamEvent {
        group: json["parent_tool_use_id"].as_str().map(str::to_owned),
        kind,
        text,
    })
}

fn content_summary(content: &Value) -> String {
    let parts: Vec<String> = content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| match part["type"].as_str()? {
            "text" => part["text"].as_str().map(first_line),
            "tool_use" => part["name"].as_str().map(str::to_owned),
            "tool_result" => Some("tool result".to_owned()),
            _ => None,
        })
        .filter(|p| !p.is_empty())
        .collect();
    parts.join(" · ")
}

fn result_summary(json: &Value) -> String {
    let subtype = json["subtype"].as_str().unwrap_or("finished");
    match json["num_turns"].as_u64() {
        Some(turns) => format!("{subtype} after {turns} turns"),
        None => subtype.to_owned(),
    }
}

/// The first line of `text`, trimmed and shortened.
fn first_line(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or_default();
    match line.char_indices().nth(SUMMARY_LEN) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}
