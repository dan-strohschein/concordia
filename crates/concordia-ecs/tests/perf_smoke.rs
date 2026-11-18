//! Perf smoke test: guards against pathological regressions, not a benchmark (M0 phase 0.3).

mod common;

use std::time::{Duration, Instant};

use common::{Pos, Vel, moving_world};

const BUDGET: Duration = Duration::from_millis(25);

#[test]
#[cfg_attr(debug_assertions, ignore = "release only: just perf-smoke")]
fn perf_smoke_iterate_1m_transform_velocity() {
    let mut world = moving_world(1_000_000);
    let mut query = world.query::<(&mut Pos, &Vel)>().unwrap();
    let mut samples: Vec<Duration> = (0..20)
        .map(|_| {
            let start = Instant::now();
            query.for_each(&mut world, |_, (p, v)| {
                p.x += v.x * 0.016;
                p.y += v.y * 0.016;
                p.z += v.z * 0.016;
            });
            start.elapsed()
        })
        .collect();
    samples.sort();
    let median = samples[samples.len() / 2];
    assert!(median < BUDGET, "median {median:?} exceeds {BUDGET:?}");
}
