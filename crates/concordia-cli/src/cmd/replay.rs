//! `concordia replay record|run|migrate|diff`: replays headless, in process (ADR-0072).

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use concordia_api::Engine;
use concordia_core::id::SessionId;
use serde_json::{Value, json};

use super::{api_failure, project_engine_on};
use crate::Failure;

/// Who the CLI records as.
const ACTOR: &str = "human:local";

/// `concordia replay` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct ReplayArgs {
    #[command(subcommand)]
    command: ReplayCommand,
}

#[derive(Debug, clap::Subcommand)]
enum ReplayCommand {
    /// Record a scripted scenario: open the scene, inject the inputs, step, write the replay.
    Record(RecordArgs),
    /// Replay a file on its scene and check its checkpoints (exit 1 on a mismatch or divergence).
    Run(RunArgs),
    /// Rewrite a version 0 replay as version 1, in place.
    Migrate(MigrateArgs),
    /// Compare two replays offline: header fields, the first differing tick, parts and fields
    /// (exit 1 if they differ).
    Diff(DiffArgs),
}

/// `concordia replay diff` arguments.
#[derive(Debug, clap::Args)]
struct DiffArgs {
    /// The expected replay (project-relative).
    a: String,
    /// The replay to compare with it.
    b: String,
    /// Project root (its .concordia/ddc holds the stills).
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

/// `concordia replay migrate` arguments.
#[derive(Debug, clap::Args)]
struct MigrateArgs {
    /// The replay to rewrite (project-relative).
    path: String,
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

/// `concordia replay record` arguments.
#[derive(Debug, clap::Args)]
struct RecordArgs {
    /// The replay to write (project-relative, ending in .replay.ron).
    path: String,
    /// Ticks to simulate.
    #[arg(long)]
    ticks: u32,
    /// Scene to open (default: [scene] main).
    #[arg(long)]
    scene: Option<String>,
    /// A JSON file of input.inject events: [{"tick": 3, "event": {...}}, ...].
    #[arg(long)]
    inputs: Option<PathBuf>,
    /// Ticks between checkpoint hashes.
    #[arg(long)]
    checkpoint_every: Option<u32>,
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

/// `concordia replay run` arguments.
#[derive(Debug, clap::Args)]
struct RunArgs {
    /// The replay to run (project-relative).
    path: String,
    /// verify (default) or resimulate.
    #[arg(long, default_value = "verify")]
    mode: String,
    /// Run even if the engine, scene, seed or code differ from the recording.
    #[arg(long)]
    force: bool,
    /// With --mode resimulate: rewrite the file on current code (re-bless; never in CI).
    #[arg(long)]
    update: bool,
    /// Job system threads.
    #[arg(long)]
    threads: Option<NonZeroUsize>,
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

pub(crate) fn run(args: &ReplayArgs) -> Result<(), Failure> {
    match &args.command {
        ReplayCommand::Record(a) => record(a),
        ReplayCommand::Run(a) => replay(a),
        ReplayCommand::Migrate(a) => migrate(a),
        ReplayCommand::Diff(a) => diff(a),
    }
}

fn record(args: &RecordArgs) -> Result<(), Failure> {
    let project =
        concordia_app::Project::load(&args.project).map_err(|e| Failure::usage(e.to_string()))?;
    let scene = args
        .scene
        .clone()
        .or_else(|| project.manifest.scene.main.clone())
        .ok_or_else(|| Failure::usage("no scene: pass --scene or set [scene] main"))?;
    let events: Value = match &args.inputs {
        Some(file) => read_json(&args.project.join(file))?,
        None => json!([]),
    };
    let mut cli = Cli::open(&args.project, None, &scene)?;
    cli.call(
        "replay.record",
        json!({"path": args.path, "checkpoint_every": args.checkpoint_every}),
    )?;
    if events.as_array().is_some_and(|e| !e.is_empty()) {
        cli.call("input.inject", json!({ "events": events }))?;
    }
    cli.call("sim.step", json!({ "ticks": args.ticks }))?;
    let stopped = cli.call("replay.record", json!({"action": "stop"}))?;
    println!("{stopped}");
    Ok(())
}

/// Prints how two replay files differ; fails if they do.
fn diff(args: &DiffArgs) -> Result<(), Failure> {
    let read = |rel: &str| -> Result<concordia_replay::Replay, Failure> {
        let file = args.project.join(rel);
        let text = std::fs::read_to_string(&file)
            .map_err(|e| Failure::usage(format!("{}: {e}", file.display())))?;
        concordia_replay::read_replay(&text).map_err(|e| Failure::usage(format!("{rel}: {e}")))
    };
    let (a, b) = (read(&args.a)?, read(&args.b)?);
    let found = concordia_replay::compare_replays(&a, &b, &args.project.join(".concordia/ddc"));
    let text = serde_json::to_string(&found).map_err(|e| Failure::usage(e.to_string()))?;
    println!("{text}");
    if found.same() {
        Ok(())
    } else {
        Err(Failure::usage(format!("{} and {} differ", args.a, args.b)))
    }
}

/// Reads any supported version and writes version 1 back, unless it already is.
fn migrate(args: &MigrateArgs) -> Result<(), Failure> {
    let file = args.project.join(&args.path);
    let text = std::fs::read_to_string(&file)
        .map_err(|e| Failure::usage(format!("{}: {e}", file.display())))?;
    let replay = concordia_replay::read_replay(&text)
        .map_err(|e| Failure::usage(format!("{}: {e}", args.path)))?;
    let written = concordia_replay::write_replay(&replay)
        .map_err(|e| Failure::usage(format!("{}: {e}", args.path)))?;
    if written == text {
        println!("{}: already version {}", args.path, replay.version);
        return Ok(());
    }
    std::fs::write(&file, written)
        .map_err(|e| Failure::usage(format!("{}: {e}", file.display())))?;
    println!("{}: rewritten as version {}", args.path, replay.version);
    Ok(())
}

fn replay(args: &RunArgs) -> Result<(), Failure> {
    let file = args.project.join(&args.path);
    let text = std::fs::read_to_string(&file)
        .map_err(|e| Failure::usage(format!("{}: {e}", file.display())))?;
    let header = concordia_replay::read_replay(&text)
        .map_err(|e| Failure::usage(format!("{}: {e}", args.path)))?
        .header;
    let mut cli = Cli::open(&args.project, args.threads, &header.scene)?;
    let result = cli
        .call(
            "replay.run",
            json!({"path": args.path, "mode": args.mode, "force": args.force, "update": args.update}),
        )
        .map_err(|f| Failure::usage(f.message.replace("\"force\": true", "--force")))?;
    println!("{result}");
    match result["ok"].as_bool() {
        Some(true) => Ok(()),
        _ => Err(Failure::usage(format!(
            "replay diverged at tick {} in {}",
            result["divergence"]["tick"], result["divergence"]["parts"]
        ))),
    }
}

/// An in-process engine with a session as the local human and `scene` open.
struct Cli {
    engine: Engine,
    session: SessionId,
}

impl Cli {
    fn open(root: &Path, threads: Option<NonZeroUsize>, scene: &str) -> Result<Self, Failure> {
        let mut engine = project_engine_on(root, threads)?;
        let session = engine.open_session();
        let mut cli = Self { engine, session };
        cli.call(
            "session.open",
            json!({"client": "concordia-cli", "actor": ACTOR}),
        )?;
        cli.call("scene.open", json!({ "path": scene }))?;
        Ok(cli)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        self.engine
            .call(self.session, method, params)
            .map_err(|e| api_failure(&e))
    }
}

fn read_json(file: &Path) -> Result<Value, Failure> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| Failure::usage(format!("{}: {e}", file.display())))?;
    serde_json::from_str(&text).map_err(|e| Failure::usage(format!("{}: {e}", file.display())))
}
