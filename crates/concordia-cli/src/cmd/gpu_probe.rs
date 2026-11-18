//! `concordia gpu-probe`: print the GPU's capabilities as JSON (CI logs and ADR-0042).

use concordia_rhi::{DeviceCaps, Requirement, RhiError};
use serde_json::{Value, json};

use crate::Failure;

/// `concordia gpu-probe` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Exit with code 1 unless the device meets M1's rendering requirements.
    #[arg(long)]
    require_m1: bool,
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    let probed = concordia_rhi_metal::probe();
    let text = serde_json::to_string_pretty(&report(&probed))
        .map_err(|e| Failure::usage(e.to_string()))?;
    println!("{text}");
    match (args.require_m1, verdict(&probed)) {
        (true, Err(why)) => Err(Failure::usage(format!("GPU does not meet M1: {why}"))),
        _ => Ok(()),
    }
}

fn report(probed: &Result<DeviceCaps, RhiError>) -> Value {
    match probed {
        Ok(caps) => json!({
            "available": true,
            "caps": caps,
            "missing_for_m1": caps.missing(Requirement::M1),
        }),
        Err(e) => json!({
            "available": false,
            "reason": e.to_string(),
            "missing_for_m1": ["a GPU device"],
        }),
    }
}

fn verdict(probed: &Result<DeviceCaps, RhiError>) -> Result<(), String> {
    let caps = probed.as_ref().map_err(ToString::to_string)?;
    match caps.missing(Requirement::M1) {
        missing if missing.is_empty() => Ok(()),
        missing => Err(missing.join(", ")),
    }
}
