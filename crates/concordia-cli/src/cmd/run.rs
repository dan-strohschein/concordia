//! `concordia run`: open the project's window on the main thread.

use std::path::PathBuf;
use std::time::Duration;

use concordia_app::Project;
use concordia_platform::{Flow, RunOptions, WindowApp, WindowEvent, run_window};

use crate::Failure;

/// `concordia run` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
    /// Close the window after this many milliseconds (smoke tests).
    #[arg(long)]
    exit_after_ms: Option<u64>,
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    let project = Project::load(&args.project).map_err(|e| Failure::usage(e.to_string()))?;
    project
        .modules()
        .map_err(|e| Failure::usage(e.to_string()))?;
    let options = RunOptions {
        exit_after: args.exit_after_ms.map(Duration::from_millis),
    };
    run_window(&project.manifest.window, options, &mut Viewer)
        .map_err(|e| Failure::usage(e.to_string()))
}

/// Reports the window's lifecycle; input reaches the simulation once `run` drives it.
struct Viewer;

impl WindowApp for Viewer {
    fn event(&mut self, event: WindowEvent) -> Flow {
        match event {
            WindowEvent::Opened {
                width,
                height,
                scale_factor,
            } => {
                eprintln!(
                    "concordia run: window opened ({width}x{height} px, scale {scale_factor})"
                );
                Flow::Continue
            }
            WindowEvent::CloseRequested => Flow::Exit,
            _ => Flow::Continue,
        }
    }
}
