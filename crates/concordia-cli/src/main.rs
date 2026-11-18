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
    /// Create a project: concordia.toml, scenes/main.scene.ron, .gitignore and .mcp.json.
    New(cmd::new::Args),
    /// Open the project's window (simulation and rendering are wired in later M1 phases).
    Run(cmd::run::Args),
    /// Run the engine headless, serving the Engine API over stdio (default) or WebSocket.
    Headless(cmd::headless::Args),
    /// Run the MCP server on stdio (what .mcp.json launches).
    Mcp(concordia_mcp::Args),
    /// Import source files (images, glTF) into the project's derived-data cache, writing sidecars.
    Import(cmd::import::Args),
    /// Print JSON Schemas of types and methods.
    Schema(cmd::schema::Args),
    /// Rewrite scene and prefab files in canonical form (or check them with --check).
    Fmt(cmd::fmt::Args),
    /// Print the GPU's capabilities as JSON (--require-m1 fails if they fall short of M1).
    GpuProbe(cmd::gpu_probe::Args),
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
        Command::Schema(args) => cmd::schema::run(&args),
        Command::Fmt(args) => cmd::fmt::run(&args),
        Command::GpuProbe(args) => cmd::gpu_probe::run(&args),
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
