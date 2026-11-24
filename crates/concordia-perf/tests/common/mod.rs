//! Shared fixtures: the reference machine, a host, a report and a budget.
#![allow(dead_code)]

use std::collections::BTreeMap;

use concordia_perf::report::Status;
use concordia_perf::{BudgetFile, HostInfo, Limit, PerfReport, ReferenceMachine, Summary};

pub fn reference() -> ReferenceMachine {
    ReferenceMachine {
        name: "author-m3-pro".into(),
        device_name: "Apple M3 Pro".into(),
        gpu_cores: 18,
        memory_gb: 18,
        os: "macOS".into(),
        os_major: 26,
        families: [
            "apple6", "apple7", "apple8", "apple9", "common3", "mac2", "metal3", "metal4",
        ]
        .map(String::from)
        .to_vec(),
    }
}

/// A host that is exactly the reference machine.
pub fn reference_host() -> HostInfo {
    let r = reference();
    HostInfo {
        device_name: r.device_name,
        gpu_cores: Some(r.gpu_cores),
        memory_gb: Some(r.memory_gb),
        os: r.os,
        os_version: "26.2".into(),
        families: r.families,
    }
}

pub fn summary(median_ms: f64, p95_ms: f64) -> Summary {
    Summary {
        median_ms,
        p95_ms,
        min_ms: median_ms * 0.9,
        max_ms: p95_ms * 1.1,
    }
}

pub fn report(host: HostInfo, passes: &[(&str, f64, f64)]) -> PerfReport {
    PerfReport {
        scene: "m1-gate".into(),
        host,
        frames: 300,
        frame: summary(4.0, 4.5),
        passes: passes
            .iter()
            .map(|(n, m, p)| ((*n).to_owned(), summary(*m, *p)))
            .collect(),
        status: Status::no_budget,
        notes: Vec::new(),
    }
}

pub fn limit(median_ms: Option<f64>, p95_ms: Option<f64>) -> Limit {
    Limit { median_ms, p95_ms }
}

pub fn budget(passes: &[(&str, Limit)]) -> BudgetFile {
    BudgetFile {
        scene: "m1-gate".into(),
        frame: limit(Some(5.0), Some(6.0)),
        passes: passes
            .iter()
            .map(|(n, l)| ((*n).to_owned(), *l))
            .collect::<BTreeMap<_, _>>(),
    }
}
