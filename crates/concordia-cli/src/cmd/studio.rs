//! `concordia studio init|roster|status` and `concordia task ...`: thin wrappers over the Studio methods.

use std::path::{Path, PathBuf};

use concordia_api::Engine;
use serde_json::{Value, json};

use super::{api_failure, project_engine};
use crate::Failure;

const HUMAN: &str = "human:local";

/// `concordia studio` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct StudioArgs {
    /// Project root.
    #[arg(long, default_value = ".", global = true)]
    project: PathBuf,
    #[command(subcommand)]
    command: StudioCommand,
}

#[derive(Debug, clap::Subcommand)]
enum StudioCommand {
    /// Add the default Agent Studio files and a [studio] section to this project, then sync.
    Init {
        /// Skip generating the Claude Code files.
        #[arg(long)]
        no_sync: bool,
    },
    /// Generate the Claude Code files (.claude/, .mcp.json, CLAUDE.md block) from studio/.
    Sync {
        /// Exit 1 with a diff if anything is out of date; write nothing.
        #[arg(long)]
        check: bool,
        /// List what would change; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Overwrite generated files that were edited by hand.
        #[arg(long)]
        force: bool,
    },
    /// Claude Code hook entry point (reads the hook's JSON on stdin). Until enforcement lands
    /// (M3 phase 3.5) it decides nothing, so Claude Code's own permission rules apply.
    Hook {
        /// pre-tool-use, post-tool-use, subagent-start or subagent-stop.
        event: String,
    },
    /// Print the resolved team (enabled roles with tools and paths, disabled roles with reasons).
    Roster,
    /// Print task counts and tasks waiting for approval.
    Status,
}

/// `concordia task` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct TaskArgs {
    /// Project root.
    #[arg(long, default_value = ".", global = true)]
    project: PathBuf,
    /// Who acts (default human:local).
    #[arg(long, default_value = HUMAN, global = true)]
    actor: String,
    #[command(subcommand)]
    command: TaskCommand,
}

#[derive(Debug, clap::Subcommand)]
enum TaskCommand {
    /// List tasks.
    List {
        /// Only this status.
        #[arg(long)]
        status: Option<String>,
        /// Only this role.
        #[arg(long)]
        role: Option<String>,
    },
    /// Show a task and its history.
    Show {
        /// Task id, e.g. T-0001.
        id: String,
    },
    /// Create a task.
    Create {
        /// Title.
        #[arg(long)]
        title: String,
        /// The role that does it.
        #[arg(long)]
        role: String,
        /// Description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Claim a task (in_progress, assigned to the actor).
    Claim {
        /// Task id.
        id: String,
    },
    /// Change a task's status.
    Update {
        /// Task id.
        id: String,
        /// New status.
        #[arg(long)]
        status: String,
    },
    /// Add a note.
    Comment {
        /// Task id.
        id: String,
        /// The note.
        text: String,
    },
    /// Approve a task in review.
    Approve {
        /// Task id.
        id: String,
        /// What was checked.
        #[arg(long)]
        comment: Option<String>,
    },
    /// Reject a task in review.
    Reject {
        /// Task id.
        id: String,
        /// What is wrong.
        #[arg(long)]
        comment: String,
    },
}

pub(crate) fn run_studio(args: &StudioArgs) -> Result<(), Failure> {
    match &args.command {
        StudioCommand::Init { no_sync } => {
            init(&args.project)?;
            match no_sync {
                true => Ok(()),
                false => sync(&args.project, false, false, false),
            }
        }
        StudioCommand::Sync {
            check,
            dry_run,
            force,
        } => sync(&args.project, *check, *dry_run, *force),
        StudioCommand::Hook { event } => hook(event),
        StudioCommand::Roster => call(&args.project, "studio.roster", json!({})),
        StudioCommand::Status => call(&args.project, "studio.status", json!({})),
    }
}

pub(crate) fn run_task(args: &TaskArgs) -> Result<(), Failure> {
    let actor = &args.actor;
    let (method, params) = match &args.command {
        TaskCommand::List { status, role } => {
            ("task.list", json!({"status": status, "role": role}))
        }
        TaskCommand::Show { id } => ("task.get", json!({"id": id})),
        TaskCommand::Create {
            title,
            role,
            description,
        } => (
            "task.create",
            json!({"title": title, "role": role, "description": description, "actor": actor}),
        ),
        TaskCommand::Claim { id } => ("task.claim", json!({"id": id, "actor": actor})),
        TaskCommand::Update { id, status } => (
            "task.update",
            json!({"id": id, "status": status, "actor": actor}),
        ),
        TaskCommand::Comment { id, text } => (
            "task.comment",
            json!({"id": id, "text": text, "actor": actor}),
        ),
        TaskCommand::Approve { id, comment } => (
            "task.approve",
            json!({"id": id, "comment": comment, "actor": actor}),
        ),
        TaskCommand::Reject { id, comment } => (
            "task.reject",
            json!({"id": id, "comment": comment, "actor": actor}),
        ),
    };
    call(&args.project, method, without_nulls(params))
}

/// Calls one Studio method on an in-process engine and prints the result as JSON.
fn call(project: &Path, method: &str, params: Value) -> Result<(), Failure> {
    let mut engine = studio_engine(project)?;
    let session = engine.open_session();
    let result = engine
        .call(session, method, params)
        .map_err(|e| api_failure(&e))?;
    let text = serde_json::to_string_pretty(&result).map_err(|e| Failure::usage(e.to_string()))?;
    println!("{text}");
    Ok(())
}

fn studio_engine(project: &Path) -> Result<Engine, Failure> {
    let manifest = concordia_app::Project::load(project)
        .map_err(|e| Failure::usage(e.to_string()))?
        .manifest;
    if manifest.studio.is_none() {
        return Err(Failure::usage(
            "this project has no [studio] section in concordia.toml; run `concordia studio init` to add Agent Studio",
        ));
    }
    project_engine(project)
}

/// Writes the default Studio files and adds `[studio]` to `concordia.toml`.
fn init(project: &Path) -> Result<(), Failure> {
    let files = concordia_studio::init(project, Path::new("studio"))
        .map_err(|e| Failure::usage(e.to_string()))?;
    let manifest_path = project.join("concordia.toml");
    let manifest = concordia_app::Project::load(project)
        .map_err(|e| Failure::usage(e.to_string()))?
        .manifest;
    if manifest.studio.is_none() {
        let mut text = std::fs::read_to_string(&manifest_path).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("\n[studio]\n");
        std::fs::write(&manifest_path, text)
            .map_err(|e| Failure::usage(format!("{}: {e}", manifest_path.display())))?;
    }
    eprintln!("added Agent Studio: {} files under studio/", files.len());
    Ok(())
}

/// Runs `studio.sync` and reports it; `--check` fails when anything is stale.
pub(crate) fn sync(project: &Path, check: bool, dry_run: bool, force: bool) -> Result<(), Failure> {
    let mut engine = studio_engine(project)?;
    let session = engine.open_session();
    let params = json!({"check": check || dry_run, "force": force, "actor": HUMAN});
    let report = engine
        .call(session, "studio.sync", params)
        .map_err(|e| api_failure(&e))?;
    let verb = |action: &str| match (check || dry_run, action) {
        (true, "create") => "would create",
        (true, "update") => "would update",
        (true, "delete") => "would delete",
        (_, "create") => "created",
        (_, "update") => "updated",
        (_, _) => "deleted",
    };
    for change in report["changes"].as_array().into_iter().flatten() {
        let action = change["action"].as_str().unwrap_or_default();
        if action == "unchanged" {
            continue;
        }
        println!(
            "{} {}",
            verb(action),
            change["path"].as_str().unwrap_or_default()
        );
        if let (true, Some(diff)) = (check || dry_run, change["diff"].as_str()) {
            print!("{diff}");
        }
    }
    let refused: Vec<&str> = report["refused"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    match (check && report["stale"] == true, refused.is_empty()) {
        (true, _) => Err(Failure {
            code: 1,
            message: "the Claude Code files are out of date; run `concordia studio sync`".into(),
        }),
        (false, false) => Err(Failure {
            code: 1,
            message: format!(
                "left hand-edited generated files alone: {}; edit studio/ instead, or pass --force to overwrite them",
                refused.join(", ")
            ),
        }),
        (false, true) => Ok(()),
    }
}

/// The hook entry point: consumes the hook input and makes no decision (phase 3.5 adds enforcement).
fn hook(event: &str) -> Result<(), Failure> {
    const EVENTS: [&str; 4] = [
        "pre-tool-use",
        "post-tool-use",
        "subagent-start",
        "subagent-stop",
    ];
    if !EVENTS.contains(&event) {
        return Err(Failure::usage(format!(
            "unknown hook event {event:?}; expected one of {}",
            EVENTS.join(", ")
        )));
    }
    let mut input = Vec::new();
    std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)
        .map_err(|e| Failure::usage(format!("reading the hook input: {e}")))?;
    Ok(())
}

fn without_nulls(mut params: Value) -> Value {
    if let Value::Object(map) = &mut params {
        map.retain(|_, v| !v.is_null());
    }
    params
}
