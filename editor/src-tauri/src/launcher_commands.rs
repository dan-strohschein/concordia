//! Tauri commands for the Claude Code launcher (ADR-0083). The UI's Studio panel calls these; the
//! headless stream arrives as `launcher://event` events and ends with `launcher://done`.

use std::path::Path;

use tauri::{AppHandle, Emitter, State};

use crate::claude_launcher::{Detection, Launcher, RunSlot};

/// The launcher and its running headless session, if any.
#[derive(Debug)]
pub struct LauncherState {
    launcher: Launcher,
    run: RunSlot,
}

impl LauncherState {
    /// A launcher for `project` whose children get this process's environment unchanged.
    pub fn new(project: &Path) -> Self {
        let project = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_owned());
        Self {
            launcher: Launcher::new(&project, std::env::vars_os().collect()),
            run: RunSlot::default(),
        }
    }
}

/// How the run ended.
#[derive(Debug, Clone, serde::Serialize)]
struct Done {
    code: Option<i32>,
    error: Option<String>,
}

/// Finds `claude` and its login method.
#[tauri::command]
pub fn launcher_detect(state: State<'_, LauncherState>) -> Detection {
    state.launcher.detect()
}

/// Opens `app` (e.g. Terminal) running `claude --agent director` in the project.
#[tauri::command]
pub fn launcher_open_terminal(state: State<'_, LauncherState>, app: String) -> Result<(), String> {
    state
        .launcher
        .open_terminal(&app)
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Starts a headless Director run; refuses while one is going.
#[tauri::command]
pub fn launcher_run(
    state: State<'_, LauncherState>,
    handle: AppHandle,
    prompt: String,
    max_budget_usd: Option<f64>,
) -> Result<(), String> {
    if state.run.busy() {
        return Err("a headless run is already going; stop it first".into());
    }
    crate::claude_launcher::headless_argv(&prompt, max_budget_usd).map_err(|e| e.to_string())?;
    let (launcher, run) = (state.launcher.clone(), state.run.clone());
    std::thread::Builder::new()
        .name("concordia-launcher".into())
        .spawn(move || {
            let result = launcher.run_headless_in(&run, &prompt, max_budget_usd, |event| {
                let _ = handle.emit("launcher://event", event);
            });
            let done = match result {
                Ok(status) => Done {
                    code: status.code(),
                    error: None,
                },
                Err(e) => Done {
                    code: None,
                    error: Some(e.to_string()),
                },
            };
            let _ = handle.emit("launcher://done", done);
        })
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Stops the headless run; true if one was going.
#[tauri::command]
pub fn launcher_stop(state: State<'_, LauncherState>) -> bool {
    state.run.stop()
}
