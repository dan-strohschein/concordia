//! `concordia gpu-probe`: print the GPU's capabilities as JSON (CI logs, ADR-0042, ADR-0088).

use concordia_rhi::{Backend, Requirement, RhiCaps, RhiError};
use concordia_rhi_select::BackendChoice;
use serde_json::{Value, json};

use crate::Failure;

/// `concordia gpu-probe` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Exit with code 1 unless the device meets M1's rendering requirements.
    #[arg(long)]
    require_m1: bool,
    /// Print the raw probe record (families, sampling points, …) instead; a test fixture.
    #[arg(long, conflicts_with = "require_m1")]
    record: bool,
    /// The backend to probe: auto (Metal on macOS, Vulkan elsewhere), metal or vulkan
    /// (default: `CONCORDIA_BACKEND`, else auto).
    #[arg(long)]
    backend: Option<BackendChoice>,
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    let backend = match args.backend {
        Some(choice) => choice.resolve(),
        None => concordia_rhi_select::choose_from_env(BackendChoice::Auto)
            .map_err(|e| Failure::usage(e.to_string()))?,
    };
    if args.record {
        return print_record(backend);
    }
    let probed = concordia_rhi_select::probe(backend);
    let text = serde_json::to_string_pretty(&report(backend, &probed))
        .map_err(|e| Failure::usage(e.to_string()))?;
    println!("{text}");
    match (args.require_m1, verdict(&probed)) {
        (true, Err(why)) => Err(Failure::usage(format!("GPU does not meet M1: {why}"))),
        _ => Ok(()),
    }
}

fn report(backend: Backend, probed: &Result<RhiCaps, RhiError>) -> Value {
    match probed {
        Ok(caps) => json!({
            "backend": backend,
            "available": true,
            "caps": caps,
            "missing_for_m1": caps.missing(Requirement::M1),
        }),
        Err(e) => json!({
            "backend": backend,
            "available": false,
            "reason": e.to_string(),
            "missing_for_m1": ["a GPU device"],
        }),
    }
}

fn verdict(probed: &Result<RhiCaps, RhiError>) -> Result<(), String> {
    let caps = probed.as_ref().map_err(ToString::to_string)?;
    match caps.missing(Requirement::M1) {
        missing if missing.is_empty() => Ok(()),
        missing => Err(missing.join(", ")),
    }
}

fn print_record(backend: Backend) -> Result<(), Failure> {
    let record = concordia_rhi_select::probe_record(backend)
        .map_err(|e| Failure::usage(format!("no probe record: {e}")))?;
    let text = serde_json::to_string_pretty(&record).map_err(|e| Failure::usage(e.to_string()))?;
    println!("{text}");
    Ok(())
}
