//! `concordia`: create projects, run the engine headless or as an MCP server, print schemas, format scenes, probe the GPU, open a window.
//!
//! Every subcommand is a thin wrapper over `concordia-api`, `concordia-scene` or `concordia-mcp`.

mod cmd;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The Concordia game engine command line.
#[derive(Debug, Parser)]
#[command(name = "concordia", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a project: concordia.toml, scenes/main.scene.ron, .gitignore, .mcp.json and Agent Studio.
    New(cmd::new::Args),
    /// Open the project's window and play its main scene there (`--edit`: show the edit world).
    Run(cmd::run::Args),
    /// Run the engine headless, serving the Engine API over stdio (default) or WebSocket.
    Headless(cmd::headless::Args),
    /// Run the MCP server on stdio (what .mcp.json launches).
    Mcp(concordia_mcp::Args),
    /// Import source files (images, glTF) into the project's derived-data cache, writing sidecars.
    Import(cmd::import::Args),
    /// Meshlets: `cluster stats` prints how a model's meshes were split for GPU culling.
    Cluster(cmd::cluster::ClusterArgs),
    /// Print JSON Schemas of types and methods.
    Schema(cmd::schema::Args),
    /// Rewrite scene and prefab files in canonical form (or check them with --check).
    Fmt(cmd::fmt::Args),
    /// Print the GPU's capabilities as JSON (--require-m1 fails if they fall short of M1).
    GpuProbe(cmd::gpu_probe::Args),
    /// Agent Studio: add it to a project, show the team and board status.
    Studio(cmd::studio::StudioArgs),
    /// The Agent Studio task board: list, show, create, claim, update, comment, approve, reject.
    Task(cmd::studio::TaskArgs),
    /// Record a scripted scenario to a .replay.ron file, or replay one and check its checkpoints.
    Replay(cmd::replay::ReplayArgs),
    /// Check playtest assertions over a replay, a playtest or the scene now.
    Assert(cmd::assert::AssertArgs),
    /// Run the project's tests: playtests.
    Test(cmd::assert::TestArgs),
    /// Measure a perf scene's GPU time per pass and judge it against its budget (`perf run`).
    Perf(cmd::perf::PerfArgs),
}

/// A failure with a process exit code.
#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) code: u8,
    pub(crate) message: String,
}

impl Failure {
    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: message.into(),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::New(args) => cmd::new::run(&args),
        Command::Run(args) => cmd::run::run(&args),
        Command::Headless(args) => cmd::block_on(cmd::headless::run(args)),
        Command::Mcp(args) => cmd::block_on(async move {
            concordia_mcp::run(args)
                .await
                .map_err(|e| Failure::usage(e.to_string()))
        }),
        Command::Import(args) => cmd::import::run(&args),
        Command::Cluster(args) => cmd::cluster::run(&args),
        Command::Schema(args) => cmd::schema::run(&args),
        Command::Fmt(args) => cmd::fmt::run(&args),
        Command::GpuProbe(args) => cmd::gpu_probe::run(&args),
        Command::Studio(args) => cmd::studio::run_studio(&args),
        Command::Task(args) => cmd::studio::run_task(&args),
        Command::Replay(args) => cmd::replay::run(&args),
        Command::Assert(args) => cmd::assert::run_assert(&args),
        Command::Test(args) => cmd::assert::run_test(&args),
        Command::Perf(args) => cmd::perf::run(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            if !failure.message.is_empty() {
                eprintln!("concordia: {}", failure.message);
            }
            ExitCode::from(failure.code)
        }
    }
}
