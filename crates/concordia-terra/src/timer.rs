//! Timers in scaled simulation time (M2 phase 2.0): a timer fires on the tick its time runs out.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::EcsError;
use crate::processor::{Phase, Processor, View, processor};
use crate::schedule::SystemCtx;

/// Simulated time this close to zero counts as expired (sums of fixed steps aren't exact).
const EPSILON: f64 = 1e-9;

/// Counts down simulated time; fires on the tick it expires (a repeating one every period).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Timer {
    /// Seconds of simulated time until the next fire.
    pub remaining: f64,
    /// Seconds between fires, for a repeating timer.
    pub period: Option<f64>,
    /// How many times it fired on the latest tick.
    pub fired: u32,
    /// True once a one-shot timer has fired.
    pub done: bool,
}

fn seconds(what: &'static str, s: f32) -> Result<f64, EcsError> {
    if s.is_finite() && s > 0.0 {
        Ok(f64::from(s))
    } else {
        Err(EcsError::InvalidValue {
            what,
            expected: "a finite number of seconds above 0",
            value: s.to_string(),
        })
    }
}

impl Timer {
    /// Fires once after `seconds` of simulated time.
    pub fn once(seconds_left: f32) -> Result<Self, EcsError> {
        Ok(Self {
            remaining: seconds("timer duration", seconds_left)?,
            period: None,
            fired: 0,
            done: false,
        })
    }

    /// Fires every `seconds` of simulated time.
    pub fn repeating(period: f32) -> Result<Self, EcsError> {
        let period = seconds("timer period", period)?;
        Ok(Self {
            remaining: period,
            period: Some(period),
            fired: 0,
            done: false,
        })
    }

    /// How many times it fired on the latest tick (more than 1 when a step spans several periods).
    pub fn fired(&self) -> u32 {
        self.fired
    }

    /// True once a one-shot timer has fired.
    pub fn finished(&self) -> bool {
        self.done
    }

    /// Advances by `dt` seconds of simulated time.
    fn advance(&mut self, dt: f64) {
        self.fired = 0;
        if self.done {
            return;
        }
        self.remaining -= dt;
        while self.remaining <= EPSILON {
            self.fired += 1;
            match self.period {
                Some(period) => self.remaining += period,
                None => {
                    self.done = true;
                    self.remaining = 0.0;
                    break;
                }
            }
        }
    }
}

/// The built-in processor that advances every [`Timer`] first thing each tick.
pub(crate) fn timers_processor() -> impl Processor {
    processor("terra.timers", Phase::PrePhysics)
        .writes::<Timer>()
        .run(|view: &mut View<'_>, ctx: &SystemCtx<'_>| {
            if let Ok(query) = view.query::<&mut Timer>() {
                query.for_each(|_, timer| timer.advance(ctx.dt_exact));
            }
        })
}
