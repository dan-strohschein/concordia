//! `tests/perf/budgets/<scene>.budget.ron`: a scene's GPU budgets on the reference machine.

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::PerfError;

/// Largest median and/or 95th-percentile GPU time (ms); an absent bound is not checked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    /// Largest median (ms).
    #[serde(default)]
    pub median_ms: Option<f64>,
    /// Largest 95th percentile (ms).
    #[serde(default)]
    pub p95_ms: Option<f64>,
}

/// A scene's budgets: the whole frame and each named pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BudgetFile {
    /// The perf scene these budgets are for.
    pub scene: String,
    /// The whole frame's GPU time.
    pub frame: Limit,
    /// Each pass's GPU time, by pass name (`render.stats` `passes`).
    pub passes: BTreeMap<String, Limit>,
}

impl BudgetFile {
    /// Reads and validates a budget file's text.
    pub fn read(text: &str) -> Result<Self, PerfError> {
        let file: Self =
            concordia_scene::read_document(text).map_err(|e| PerfError::Invalid(e.to_string()))?;
        file.validate()?;
        Ok(file)
    }

    /// The canonical text of this budget file.
    pub fn to_text(&self) -> Result<String, PerfError> {
        self.validate()?;
        concordia_scene::write_document(self).map_err(|e| PerfError::Invalid(e.to_string()))
    }

    /// Every limit has a bound, and every bound is a positive, finite time.
    pub fn validate(&self) -> Result<(), PerfError> {
        self.frame.validate("frame")?;
        for (pass, limit) in &self.passes {
            limit.validate(pass)?;
        }
        Ok(())
    }
}

impl Limit {
    /// The bounds that are set, with their statistic.
    pub fn bounds(&self) -> impl Iterator<Item = (crate::Stat, f64)> {
        [
            (crate::Stat::Median, self.median_ms),
            (crate::Stat::P95, self.p95_ms),
        ]
        .into_iter()
        .filter_map(|(stat, ms)| Some((stat, ms?)))
    }

    fn validate(&self, name: &str) -> Result<(), PerfError> {
        if self.bounds().next().is_none() {
            return Err(PerfError::Invalid(format!(
                "budget {name:?} sets neither median_ms nor p95_ms"
            )));
        }
        match self.bounds().find(|(_, ms)| !(ms.is_finite() && *ms > 0.0)) {
            Some((_, ms)) => Err(PerfError::Invalid(format!(
                "budget {name:?}: {ms} ms is not a positive time"
            ))),
            None => Ok(()),
        }
    }
}
