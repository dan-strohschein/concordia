//! `tests/perf/reference.ron`: the reference machine, and what this host is.

use concordia_reflect::Reflect;
use concordia_rhi::RhiCaps;
use serde::{Deserialize, Serialize};

use crate::PerfError;

/// The machine perf budgets hold on (the author's M3 Pro).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ReferenceMachine {
    /// A short name for reports.
    pub name: String,
    /// The GPU as the driver names it (`RhiCaps::device_name`).
    pub device_name: String,
    /// GPU cores.
    pub gpu_cores: u32,
    /// Memory (GB).
    pub memory_gb: u32,
    /// The OS (`macOS`).
    pub os: String,
    /// The OS's major version.
    pub os_major: u32,
    /// GPU feature sets (`RhiCaps::feature_sets`), sorted.
    pub families: Vec<String>,
}

/// What this host is, as far as perf is concerned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct HostInfo {
    /// The GPU's name.
    pub device_name: String,
    /// GPU cores, if known.
    pub gpu_cores: Option<u32>,
    /// Memory (GB), if known.
    pub memory_gb: Option<u32>,
    /// The OS.
    pub os: String,
    /// The OS version (e.g. `26.2`).
    pub os_version: String,
    /// GPU feature sets, sorted.
    pub families: Vec<String>,
}

impl ReferenceMachine {
    /// Reads a reference file's text.
    pub fn read(text: &str) -> Result<Self, PerfError> {
        concordia_scene::read_document(text).map_err(|e| PerfError::Invalid(e.to_string()))
    }

    /// Why `host` is not this machine (None when it is).
    pub fn mismatch(&self, host: &HostInfo) -> Option<String> {
        let shown = |v: Option<u32>| v.map_or_else(|| "unknown".to_owned(), |v| v.to_string());
        let mut why = Vec::new();
        if host.device_name != self.device_name {
            why.push(format!(
                "GPU {} (reference: {})",
                host.device_name, self.device_name
            ));
        }
        if host.gpu_cores != Some(self.gpu_cores) {
            why.push(format!(
                "{} GPU cores (reference: {})",
                shown(host.gpu_cores),
                self.gpu_cores
            ));
        }
        if host.memory_gb != Some(self.memory_gb) {
            why.push(format!(
                "{} GB memory (reference: {} GB)",
                shown(host.memory_gb),
                self.memory_gb
            ));
        }
        if host.os != self.os || os_major(&host.os_version) != Some(self.os_major) {
            why.push(format!(
                "{} {} (reference: {} {})",
                host.os, host.os_version, self.os, self.os_major
            ));
        }
        if host.families != self.families {
            why.push(format!(
                "GPU families {} (reference: {})",
                host.families.join(" "),
                self.families.join(" ")
            ));
        }
        (!why.is_empty()).then(|| why.join("; "))
    }
}

/// The major version in `26.2` or `26`.
fn os_major(version: &str) -> Option<u32> {
    version.split('.').next()?.trim().parse().ok()
}

/// Finds out what this host is.
pub trait HostProbe {
    /// This host, given its GPU's capabilities.
    fn host(&self, caps: &RhiCaps) -> HostInfo;
}

/// Reads this host through the OS's tools (`ioreg`, `sysctl`, `sw_vers` on macOS).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemProbe;

impl HostProbe for SystemProbe {
    fn host(&self, caps: &RhiCaps) -> HostInfo {
        let mac = cfg!(target_os = "macos");
        HostInfo {
            device_name: caps.device_name.clone(),
            gpu_cores: mac
                .then(|| tool("ioreg", &["-rc", "AGXAccelerator", "-d1"]))
                .flatten()
                .and_then(|out| parse_gpu_cores(&out)),
            memory_gb: mac
                .then(|| tool("sysctl", &["-n", "hw.memsize"]))
                .flatten()
                .and_then(|out| parse_memory_gb(&out)),
            os: if mac { "macOS" } else { std::env::consts::OS }.to_owned(),
            os_version: mac
                .then(|| tool("sw_vers", &["-productVersion"]))
                .flatten()
                .map(|v| v.trim().to_owned())
                .unwrap_or_default(),
            families: caps.feature_sets.clone(),
        }
    }
}

/// A tool's standard output, if it ran and succeeded.
fn tool(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `"gpu-core-count" = 18` in `ioreg -rc AGXAccelerator -d1` output.
pub fn parse_gpu_cores(ioreg: &str) -> Option<u32> {
    let line = ioreg.lines().find(|l| l.contains("\"gpu-core-count\""))?;
    line.split('=').nth(1)?.trim().parse().ok()
}

/// Whole gigabytes (GiB) from `sysctl -n hw.memsize` bytes.
pub fn parse_memory_gb(memsize: &str) -> Option<u32> {
    let bytes: u64 = memsize.trim().parse().ok()?;
    u32::try_from((bytes + (1 << 29)) >> 30).ok()
}
