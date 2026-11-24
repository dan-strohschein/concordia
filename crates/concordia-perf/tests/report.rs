//! Reports summarize each pass over the measured frames (M4 phase 4.1a).

mod common;

use concordia_perf::report::{FrameSample, Status};
use concordia_perf::{PerfReport, Summary};

#[test]
fn summary_takes_median_and_nearest_rank_p95() {
    let samples: Vec<f64> = (1..=100).rev().map(f64::from).collect();
    let s = Summary::of(&samples).expect("a summary");
    assert_eq!(
        (s.median_ms, s.p95_ms, s.min_ms, s.max_ms),
        (50.5, 95.0, 1.0, 100.0)
    );
    let odd = Summary::of(&[3.0, 1.0, 2.0]).unwrap();
    assert_eq!((odd.median_ms, odd.p95_ms), (2.0, 3.0));
    assert_eq!(Summary::of(&[]), None);
    assert_eq!(Summary::of(&[1.0, f64::NAN]), None);
}

#[test]
fn report_summarizes_every_pass_and_the_frame() {
    let sample = |frame: f64, forward: f64| FrameSample {
        gpu_ms: frame,
        passes: vec![("forward".into(), forward), ("tonemap".into(), 0.1)],
    };
    let samples = [sample(4.0, 2.0), sample(5.0, 3.0), sample(6.0, 4.0)];
    let r = PerfReport::from_samples("m1-gate", common::reference_host(), &samples).unwrap();
    assert_eq!(r.frames, 3);
    assert_eq!(r.frame.median_ms, 5.0);
    assert_eq!(r.passes["forward"].median_ms, 3.0);
    assert_eq!(r.passes["tonemap"].p95_ms, 0.1);
    assert_eq!(r.status, Status::no_budget);
    assert!(PerfReport::from_samples("m1-gate", common::reference_host(), &[]).is_err());
}
