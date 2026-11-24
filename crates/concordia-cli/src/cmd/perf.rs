//! `concordia perf run`: measure a perf scene on this GPU, write its report and judge it against
//! its budget on the reference machine (ADR-0089).

use std::path::PathBuf;

use concordia_perf::machine::SystemProbe;
use concordia_perf::{PerfReport, RunOptions, Verdict};

use crate::Failure;

/// `concordia perf` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct PerfArgs {
    #[command(subcommand)]
    command: PerfCommand,
}

#[derive(Debug, clap::Subcommand)]
enum PerfCommand {
    /// Measure `<perf>/scenes/<name>.perf.ron`, write `<out>/<name>.perf.ron`; exit 1 over budget
    /// on the reference machine.
    Run(RunArgs),
}

/// `concordia perf run` arguments.
#[derive(Debug, clap::Args)]
struct RunArgs {
    /// The perf scene file.
    scene: PathBuf,
    /// Where to write the report.
    #[arg(long, default_value = "target/perf")]
    out: PathBuf,
    /// Warm-up frames (default: the scene's).
    #[arg(long)]
    warmup: Option<u32>,
    /// Measured frames (default: the scene's).
    #[arg(long)]
    frames: Option<u32>,
    /// Exit 2 when this is not the reference machine (gates); otherwise that only reports.
    #[arg(long)]
    require_reference: bool,
}

pub(crate) fn run(args: &PerfArgs) -> Result<(), Failure> {
    match &args.command {
        PerfCommand::Run(run) => run_scene(run),
    }
}

fn run_scene(args: &RunArgs) -> Result<(), Failure> {
    let options = RunOptions {
        warmup_frames: args.warmup,
        frames: args.frames,
    };
    let run = concordia_perf::run(&args.scene, &options, &SystemProbe)
        .map_err(|e| Failure::usage(e.to_string()))?;
    let path = run
        .write(&args.out)
        .map_err(|e| Failure::usage(e.to_string()))?;
    print!("{}", summary(&run.report));
    println!("wrote {}", path.display());
    exit_for(&run.verdict, args.require_reference)
}

/// The status line, then the frame and each pass: median and p95.
fn summary(report: &PerfReport) -> String {
    let row = |name: &str, s: &concordia_perf::Summary| {
        format!(
            "  {name:<16} median {:>7.3} ms  p95 {:>7.3} ms\n",
            s.median_ms, s.p95_ms
        )
    };
    let mut text = format!(
        "{}: {:?} ({} frames on {})\n",
        report.scene, report.status, report.frames, report.host.device_name
    );
    text += &row("frame", &report.frame);
    for (name, s) in &report.passes {
        text += &row(name, s);
    }
    text
}

/// 0 for a pass or a report-only run; 1 over budget; 2 off the reference when it is required.
fn exit_for(verdict: &Verdict, require_reference: bool) -> Result<(), Failure> {
    let lines = |v: &[concordia_perf::Violation]| {
        v.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    };
    match verdict {
        Verdict::Fail(v) => Err(Failure::usage(format!("over budget:\n{}", lines(v)))),
        Verdict::NotReference { reason, .. } if require_reference => Err(Failure {
            code: 2,
            message: format!("not the reference machine: {reason}"),
        }),
        Verdict::NotReference { reason, .. } => {
            eprintln!("concordia: report only, not the reference machine: {reason}");
            Ok(())
        }
        Verdict::NoBudget => {
            eprintln!("concordia: report only, the scene has no budget file");
            Ok(())
        }
        Verdict::Pass => Ok(()),
    }
}
