//! Comparing a report with its budgets on the reference machine.

use std::fmt;

use crate::{BudgetFile, Limit, PerfReport, ReferenceMachine, Summary};

/// Which statistic broke a budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    /// The median.
    Median,
    /// The 95th percentile.
    P95,
}

/// A pass (or `frame`) over its budget, or budgeted but never measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    /// The pass, or `frame`.
    pub pass: String,
    /// The statistic.
    pub stat: Stat,
    /// What was measured (ms); None when the pass never ran.
    pub measured_ms: Option<f64>,
    /// The budget (ms).
    pub budget_ms: f64,
}

impl fmt::Display for Stat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stat::Median => "median",
            Stat::P95 => "p95",
        })
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (pass, stat, budget) = (&self.pass, self.stat, self.budget_ms);
        match self.measured_ms {
            Some(ms) => write!(
                f,
                "{pass}: {stat} {ms:.2} ms > budget {budget:.2} ms (+{:.0}%)",
                (ms / budget - 1.0) * 100.0
            ),
            None => write!(f, "{pass}: not measured (budget {stat} {budget:.2} ms)"),
        }
    }
}

/// A run's outcome. Only `Pass` is a pass, and it needs the reference machine.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Within every budget, on the reference machine.
    Pass,
    /// Over budget on the reference machine.
    Fail(Vec<Violation>),
    /// Not the reference machine: what would have failed is reported, nothing passes.
    NotReference {
        /// How this host differs.
        reason: String,
        /// Budgets the run would have broken.
        violations: Vec<Violation>,
    },
    /// The scene has no budget file.
    NoBudget,
}

/// Every budget `report` breaks.
pub fn check(report: &PerfReport, budget: &BudgetFile) -> Vec<Violation> {
    let frame = over("frame", Some(&report.frame), &budget.frame);
    let passes = budget
        .passes
        .iter()
        .flat_map(|(name, limit)| over(name, report.passes.get(name), limit));
    frame.chain(passes).collect()
}

/// The bounds of `limit` that `measured` breaks (all of them when it never ran).
fn over<'a>(
    pass: &'a str,
    measured: Option<&'a Summary>,
    limit: &'a Limit,
) -> impl Iterator<Item = Violation> + 'a {
    limit.bounds().filter_map(move |(stat, budget_ms)| {
        let ms = measured.map(|s| match stat {
            Stat::Median => s.median_ms,
            Stat::P95 => s.p95_ms,
        });
        ms.is_none_or(|ms| ms > budget_ms).then(|| Violation {
            pass: pass.to_owned(),
            stat,
            measured_ms: ms,
            budget_ms,
        })
    })
}

/// The verdict for `report` against `budget`, on `reference`.
pub fn judge(
    report: &PerfReport,
    budget: Option<&BudgetFile>,
    reference: &ReferenceMachine,
) -> Verdict {
    let violations = budget.map(|b| check(report, b));
    match (reference.mismatch(&report.host), violations) {
        (Some(reason), violations) => Verdict::NotReference {
            reason,
            violations: violations.unwrap_or_default(),
        },
        (None, None) => Verdict::NoBudget,
        (None, Some(v)) if v.is_empty() => Verdict::Pass,
        (None, Some(v)) => Verdict::Fail(v),
    }
}
