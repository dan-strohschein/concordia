//! The online monitor: decides a temporal operator tick by tick, with early exit, and keeps the
//! closest approach for a `within` that runs out.

use std::collections::BTreeMap;

use concordia_assert::{Assertion, Cmp, Dur, Pred, Temporal};
use serde_json::Value;

use crate::assert_method::{AssertOutcome, AssertStatus};
use crate::eval::{Sample, witness};

/// What a temporal operator waits for, with every duration in ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Rule {
    Within(u64),
    Always(Option<u64>),
    Never(Option<u64>),
    Eventually,
    At(u64),
}

/// One assertion being decided.
pub(crate) struct Monitor {
    pub assertion: Assertion,
    start: u64,
    rule: Rule,
    /// Lower is better (`<`, `<=`), or higher (`>`, `>=`); none for other predicates.
    aim: Option<bool>,
    closest: Option<(f64, u64)>,
    pub outcome: Option<AssertOutcome>,
}

impl Monitor {
    /// A monitor for a checked assertion (durations already in ticks).
    pub fn new(assertion: Assertion) -> Self {
        let (start, rule) = rule(&assertion.temporal, 0);
        let aim = match &assertion.pred {
            Pred::Cmp {
                op: Cmp::Lt | Cmp::Le,
                ..
            } => Some(true),
            Pred::Cmp {
                op: Cmp::Gt | Cmp::Ge,
                ..
            } => Some(false),
            _ => None,
        };
        Self {
            assertion,
            start,
            rule,
            aim,
            closest: None,
            outcome: None,
        }
    }

    /// Whether it is decided.
    pub fn decided(&self) -> bool {
        self.outcome.is_some()
    }

    /// Takes tick `tick`'s sample (or the error reading it).
    pub fn observe(&mut self, tick: u64, sample: Result<Sample, String>) {
        if self.decided() || tick < self.start {
            return;
        }
        let sample = match sample {
            Ok(s) => s,
            Err(e) => {
                return self.decide(
                    AssertStatus::error,
                    tick,
                    BTreeMap::from([("error".into(), Value::from(e))]),
                );
            }
        };
        self.track(tick, &sample);
        let rel = tick - self.start;
        let seen = witness(&self.assertion.pred, &sample);
        let (pass, fail) = (AssertStatus::passed, AssertStatus::failed);
        match self.rule {
            Rule::Within(n) if sample.holds && rel <= n => self.decide(pass, tick, seen),
            Rule::Within(n) if rel >= n => {
                let mut w = seen;
                if let Some((v, at)) = self.closest {
                    w.insert("closest".into(), Value::from(v));
                    w.insert("closest_at".into(), Value::from(at));
                }
                self.decide(fail, tick, w);
            }
            Rule::Always(_) if !sample.holds => self.decide(fail, tick, seen),
            Rule::Never(_) if sample.holds => self.decide(fail, tick, seen),
            Rule::Always(Some(n)) | Rule::Never(Some(n)) if rel >= n => {
                self.decide(pass, tick, seen)
            }
            Rule::Eventually if sample.holds => self.decide(pass, tick, seen),
            Rule::At(n) if rel == n => {
                let status = if sample.holds { pass } else { fail };
                self.decide(status, tick, seen);
            }
            _ => {}
        }
    }

    /// The run ended after `last` ticks: `always` and `never` over the whole run pass; the rest
    /// stay undecided.
    pub fn finish(&mut self, last: u64) {
        if self.decided() {
            return;
        }
        match self.rule {
            Rule::Always(None) | Rule::Never(None) if last >= self.start => {
                self.decide(AssertStatus::passed, last, BTreeMap::new());
            }
            _ => {
                self.outcome = Some(AssertOutcome {
                    assertion: self.assertion.to_string(),
                    status: AssertStatus::undecided,
                    decided_at: None,
                    witness: BTreeMap::new(),
                });
            }
        }
    }

    fn track(&mut self, tick: u64, sample: &Sample) {
        let (Some(lower), Some(v)) = (self.aim, sample.lhs) else {
            return;
        };
        let better = self
            .closest
            .is_none_or(|(best, _)| if lower { v < best } else { v > best });
        if better {
            self.closest = Some((v, tick));
        }
    }

    fn decide(&mut self, status: AssertStatus, tick: u64, witness: BTreeMap<String, Value>) {
        self.outcome = Some(AssertOutcome {
            assertion: self.assertion.to_string(),
            status,
            decided_at: Some(tick),
            witness,
        });
    }
}

/// The start tick and the rule, unwrapping `after`.
fn rule(t: &Temporal, start: u64) -> (u64, Rule) {
    let n = |d: &Dur| match d {
        Dur::Ticks(n) => *n,
        // `check` turned seconds into ticks; anything else is a bug upstream, read as 0.
        Dur::Seconds(_) => 0,
    };
    match t {
        Temporal::Within(d) => (start, Rule::Within(n(d))),
        Temporal::Always(d) => (start, Rule::Always(d.as_ref().map(n))),
        Temporal::Never(d) => (start, Rule::Never(d.as_ref().map(n))),
        Temporal::Eventually => (start, Rule::Eventually),
        Temporal::At(d) => (start, Rule::At(n(d))),
        Temporal::After(d, inner) => rule(inner, start + n(d)),
    }
}
