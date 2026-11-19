//! The deterministic fixed-step simulation loop.

use std::time::Duration;

use concordia_core::jobs::JobSystem;
use concordia_core::time::{Accumulator, Steps, Tick, TickRate};

use crate::schedule::{Schedule, SystemCtx};
use crate::world::World;

/// Simulation time, stored as a world resource (and hashed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimClock {
    /// Ticks completed.
    pub tick: Tick,
    /// Fixed rate.
    pub rate: TickRate,
}

/// Runs a schedule at a fixed rate; time comes only from `step` counts or `advance` durations.
pub struct SimLoop {
    rate: TickRate,
    schedule: Schedule,
    acc: Accumulator,
}

impl SimLoop {
    /// A loop running `schedule` at `rate`.
    pub fn new(rate: TickRate, schedule: Schedule) -> Self {
        Self {
            rate,
            schedule,
            acc: Accumulator::new(rate),
        }
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

    /// Runs exactly `ticks` fixed steps.
    pub fn step(&mut self, world: &mut World, jobs: &JobSystem, ticks: u32) {
        let dt = 1.0 / self.rate.hz() as f32;
        for _ in 0..ticks {
            let tick = self.clock(world).tick;
            self.schedule.run(world, &SystemCtx { jobs, tick, dt });
            let clock = self.clock(world);
            clock.tick = Tick(clock.tick.0.saturating_add(1));
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
            world.insert_resource(SimClock {
                tick: Tick(0),
                rate: self.rate,
            });
        }
        world.resource_mut::<SimClock>().expect("inserted above")
    }
}
