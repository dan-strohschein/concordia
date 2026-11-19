//! Fixed-step time (M0 phase 0.1).

use std::num::NonZeroU32;
use std::time::Duration;

use concordia_core::time::{Accumulator, Tick, TickRate, TimeError};

#[test]
fn accumulator_emits_whole_steps_and_keeps_remainder() {
    let mut acc = Accumulator::new(TickRate::HZ_60);
    let steps = acc.push(Duration::from_millis(40));
    assert_eq!(steps.steps, 2);
    assert_eq!(steps.dropped, 0);
    assert_eq!(acc.remainder(), Duration::from_nanos(6_666_666));
}

#[test]
fn accumulator_has_no_drift_over_many_pushes() {
    let mut acc = Accumulator::new(TickRate::HZ_60);
    let total: u32 = (0..60_000)
        .map(|_| acc.push(Duration::from_millis(1)).steps)
        .sum();
    assert_eq!(total, 3_600, "60 s at 60 Hz is exactly 3600 steps");
    assert_eq!(acc.remainder(), Duration::ZERO);
}

#[test]
fn accumulator_clamps_catch_up() {
    let mut acc = Accumulator::new(TickRate::HZ_60);
    let steps = acc.push(Duration::from_secs(10));
    assert_eq!(steps.steps, Accumulator::DEFAULT_MAX_CATCH_UP.get());
    assert_eq!(steps.dropped, 600 - 8);

    let mut acc = Accumulator::with_max_catch_up(TickRate::HZ_60, NonZeroU32::new(3).unwrap());
    assert_eq!(acc.push(Duration::from_secs(1)).steps, 3);
}

#[test]
fn fixed_clock_tick_to_seconds_is_exact() {
    assert_eq!(Tick(60).as_secs_f64(TickRate::HZ_60), 1.0);
    assert_eq!(Tick(0).as_secs_f64(TickRate::HZ_60), 0.0);
}

#[test]
fn tick_rate_from_hz_rejects_zero() {
    assert_eq!(TickRate::from_hz(0), Err(TimeError::ZeroRate));
    assert_eq!(TickRate::from_hz(60).unwrap(), TickRate::HZ_60);
}

#[test]
fn accumulator_reports_the_time_to_the_next_step_and_the_fraction_done() {
    let mut acc = Accumulator::new(TickRate::from_hz(50).unwrap());
    assert_eq!(acc.until_next_step(), Duration::from_millis(20));
    assert_eq!(acc.fraction(), 0.0);
    acc.push(Duration::from_millis(25));
    assert_eq!(acc.until_next_step(), Duration::from_millis(15));
    assert!((acc.fraction() - 0.25).abs() < 1e-6);
}
