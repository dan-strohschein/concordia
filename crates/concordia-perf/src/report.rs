//! `target/perf/<scene>.perf.ron`: what a run measured, per pass and for the frame.

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::{HostInfo, PerfError};

/// Statistics of one series of GPU times (ms).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    /// Median.
    pub median_ms: f64,
    /// 95th percentile (nearest rank).
    pub p95_ms: f64,
    /// Fastest.
    pub min_ms: f64,
    /// Slowest.
    pub max_ms: f64,
}

impl Summary {
    /// The statistics of `samples` (None when empty or any is not finite).
    pub fn of(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() || samples.iter().any(|s| !s.is_finite()) {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let median_ms = match n % 2 {
            1 => sorted[n / 2],
            _ => (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0,
        };
        // Nearest rank: the smallest sample with at least 95% of samples at or below it.
        let rank = (0.95 * n as f64).ceil() as usize;
        Some(Self {
            median_ms,
            p95_ms: sorted[rank.clamp(1, n) - 1],
            min_ms: sorted[0],
            max_ms: sorted[n - 1],
        })
    }
}

/// One measured frame: its GPU time and each timed pass's.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameSample {
    /// The whole frame (ms).
    pub gpu_ms: f64,
    /// Each pass (name, ms), in order.
    pub passes: Vec<(String, f64)>,
}

/// Whether a run met its budgets (the report's verdict, as text).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Status {
    /// On the reference machine, within every budget.
    pass,
    /// On the reference machine, over a budget.
    fail,
    /// Not the reference machine: measured and reported, never a pass.
    not_reference,
    /// No budget file for the scene yet.
    no_budget,
}

/// A run's measurements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PerfReport {
    /// The perf scene.
    pub scene: String,
    /// Where it ran.
    pub host: HostInfo,
    /// Frames measured (after warm-up).
    pub frames: u32,
    /// The whole frame's GPU time.
    pub frame: Summary,
    /// Each pass's GPU time, by name.
    pub passes: BTreeMap<String, Summary>,
    /// The verdict.
    pub status: Status,
    /// Why: each violation, or why this is not the reference machine.
    pub notes: Vec<String>,
}

impl PerfReport {
    /// A report of `samples` (status `no_budget` until judged).
    pub fn from_samples(
        scene: &str,
        host: HostInfo,
        samples: &[FrameSample],
    ) -> Result<Self, PerfError> {
        let invalid = || PerfError::Invalid(format!("{scene}: no finite GPU times measured"));
        let frames: Vec<f64> = samples.iter().map(|s| s.gpu_ms).collect();
        let mut series: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for (name, ms) in samples.iter().flat_map(|s| &s.passes) {
            series.entry(name.clone()).or_default().push(*ms);
        }
        let passes = series
            .into_iter()
            .map(|(name, ms)| Ok((name, Summary::of(&ms).ok_or_else(invalid)?)))
            .collect::<Result<_, PerfError>>()?;
        Ok(Self {
            scene: scene.to_owned(),
            host,
            frames: u32::try_from(samples.len()).map_err(|_| invalid())?,
            frame: Summary::of(&frames).ok_or_else(invalid)?,
            passes,
            status: Status::no_budget,
            notes: Vec::new(),
        })
    }

    /// Writes `verdict` into the report's status and notes.
    pub fn record(&mut self, verdict: &crate::Verdict) {
        use crate::Verdict;
        let shown = |v: &[crate::Violation]| v.iter().map(ToString::to_string).collect::<Vec<_>>();
        (self.status, self.notes) = match verdict {
            Verdict::Pass => (Status::pass, Vec::new()),
            Verdict::Fail(v) => (Status::fail, shown(v)),
            Verdict::NotReference { reason, violations } => (
                Status::not_reference,
                std::iter::once(format!("not the reference machine: {reason}"))
                    .chain(shown(violations))
                    .collect(),
            ),
            Verdict::NoBudget => (Status::no_budget, vec!["no budget file".into()]),
        };
    }

    /// The canonical text of this report.
    pub fn to_text(&self) -> Result<String, PerfError> {
        concordia_scene::write_document(self).map_err(|e| PerfError::Invalid(e.to_string()))
    }
}
