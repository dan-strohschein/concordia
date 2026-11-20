//! The deterministic fixed-step simulation loop.

use std::time::Duration;

use concordia_core::jobs::JobSystem;
use concordia_core::time::{Accumulator, Steps, Tick, TickRate};

use crate::schedule::{Schedule, SystemCtx};
use crate::world::World;

/// How fast simulated time runs relative to ticks: 1 is normal, 0 freezes it (hit-stop).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeScale(f32);

impl TimeScale {
    /// Normal speed.
    pub const ONE: TimeScale = TimeScale(1.0);

    /// A finite, non-negative scale.
    pub fn new(scale: f32) -> Result<Self, crate::error::EcsError> {
        if scale.is_finite() && scale >= 0.0 {
            Ok(Self(scale))
        } else {
            Err(crate::error::EcsError::InvalidValue {
                what: "time scale",
                expected: "finite and at least 0 (1 is normal speed, 0 freezes simulated time)",
                value: scale.to_string(),
            })
        }
    }

    /// The scale.
    pub fn get(self) -> f32 {
        self.0
    }
}

/// Simulation time, stored as a world resource (and hashed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimClock {
    /// Ticks completed.
    pub tick: Tick,
    /// Fixed rate.
    pub rate: TickRate,
    /// Scale applied to each tick's simulated time.
    pub time_scale: TimeScale,
    /// Simulated seconds elapsed (the sum of scaled steps).
    pub time: f64,
}

impl SimClock {
    /// A clock at tick 0 running at `rate`, normal speed.
    pub fn new(rate: TickRate) -> Self {
        Self {
            tick: Tick(0),
            rate,
            time_scale: TimeScale::ONE,
            time: 0.0,
        }
    }
}

/// Runs a schedule at a fixed rate; time comes only from `step` counts or `advance` durations.
pub struct SimLoop {
    rate: TickRate,
    schedule: Schedule,
    acc: Accumulator,
}

impl SimLoop {
    /// A loop running `schedule` (plus Terra's built-in processors) at `rate` on `world`; an
    /// invalid schedule is an error.
    pub fn new(
        rate: TickRate,
        schedule: Schedule,
        world: &mut World,
    ) -> Result<Self, crate::error::ScheduleError> {
        let mut full = Schedule::new();
        full.add_processor(crate::timer::timers_processor());
        full.append(schedule);
        full.plan(world)?;
        let sim = Self {
            rate,
            schedule: full,
            acc: Accumulator::new(rate),
        };
        sim.clock(world);
        Ok(sim)
    }

    /// The fixed rate.
    pub fn rate(&self) -> TickRate {
        self.rate
    }

    /// Changes the fixed rate, keeping the schedule and the tick count; the world's clock follows.
    pub fn set_rate(&mut self, world: &mut World, rate: TickRate) {
        self.rate = rate;
        self.acc = Accumulator::new(rate);
        self.clock(world).rate = rate;
    }

    /// The schedule, for adding systems.
    pub fn schedule_mut(&mut self) -> &mut Schedule {
        &mut self.schedule
    }

    /// Runs exactly `ticks` fixed steps; each advances simulated time by the step times the
    /// clock's time scale.
    pub fn step(&mut self, world: &mut World, jobs: &JobSystem, ticks: u32) {
        let base = 1.0 / f64::from(self.rate.hz());
        for _ in 0..ticks {
            let clock = *self.clock(world);
            let dt_exact = base * f64::from(clock.time_scale.get());
            let ctx = SystemCtx {
                jobs,
                tick: clock.tick,
                dt: dt_exact as f32,
                dt_exact,
            };
            self.schedule.run(world, &ctx);
            let clock = self.clock(world);
            clock.tick = Tick(clock.tick.0.saturating_add(1));
            clock.time += dt_exact;
        }
    }

    /// Real time until the next tick is due.
    pub fn until_next_step(&self) -> Duration {
        self.acc.until_next_step()
    }

    /// How far toward the next tick the simulation is, in [0, 1) (for interpolated drawing).
    pub fn fraction(&self) -> f32 {
        self.acc.fraction()
    }

    /// Adds real elapsed time and runs the steps now due.
    pub fn advance(&mut self, world: &mut World, jobs: &JobSystem, real: Duration) -> Steps {
        let steps = self.acc.push(real);
        self.step(world, jobs, steps.steps);
        steps
    }

    fn clock<'w>(&self, world: &'w mut World) -> &'w mut SimClock {
        if world.resource::<SimClock>().is_none() {
            world.insert_resource(SimClock::new(self.rate));
        }
        world.resource_mut::<SimClock>().expect("inserted above")
    }
}
