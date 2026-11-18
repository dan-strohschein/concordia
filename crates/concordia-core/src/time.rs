//! Fixed-step simulation time with exact integer accumulation.

use std::num::NonZeroU32;
use std::time::Duration;

const NANOS_PER_SEC: u128 = 1_000_000_000;

/// Errors from time configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TimeError {
    /// A tick rate of 0 Hz was requested.
    #[error("tick rate must be at least 1 Hz")]
    ZeroRate,
}

/// Fixed simulation rate in ticks per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TickRate(NonZeroU32);

impl TickRate {
    /// 60 ticks per second.
    pub const HZ_60: Self = Self(NonZeroU32::new(60).unwrap());

    /// Creates a rate from a non-zero frequency.
    pub const fn new(hz: NonZeroU32) -> Self {
        Self(hz)
    }

    /// Creates a rate, rejecting 0 Hz.
    pub fn from_hz(hz: u32) -> Result<Self, TimeError> {
        NonZeroU32::new(hz).map(Self).ok_or(TimeError::ZeroRate)
    }

    /// Ticks per second.
    pub const fn hz(self) -> u32 {
        self.0.get()
    }
}

/// A simulation tick count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Tick(pub u64);

impl Tick {
    /// Elapsed simulated seconds at `rate`.
    pub fn as_secs_f64(self, rate: TickRate) -> f64 {
        self.0 as f64 / f64::from(rate.hz())
    }
}

/// Whole steps produced by one [`Accumulator::push`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Steps {
    /// Steps to run now.
    pub steps: u32,
    /// Steps discarded by the catch-up clamp.
    pub dropped: u32,
}

/// Converts real elapsed time into fixed steps ("Fix Your Timestep!").
///
/// ```
/// use std::time::Duration;
/// use concordia_core::time::{Accumulator, TickRate};
/// let mut acc = Accumulator::new(TickRate::HZ_60);
/// assert_eq!(acc.push(Duration::from_millis(40)).steps, 2);
/// ```
///
/// The remainder is kept in units of `ns × hz`, so there is no rounding drift.
#[derive(Debug, Clone)]
pub struct Accumulator {
    rate: TickRate,
    scaled_remainder: u128,
    max_catch_up: NonZeroU32,
}

impl Accumulator {
    /// Default cap on steps per push.
    pub const DEFAULT_MAX_CATCH_UP: NonZeroU32 = NonZeroU32::new(8).unwrap();

    /// Creates an accumulator with the default catch-up cap.
    pub fn new(rate: TickRate) -> Self {
        Self::with_max_catch_up(rate, Self::DEFAULT_MAX_CATCH_UP)
    }

    /// Creates an accumulator that emits at most `max_catch_up` steps per push.
    pub fn with_max_catch_up(rate: TickRate, max_catch_up: NonZeroU32) -> Self {
        Self {
            rate,
            scaled_remainder: 0,
            max_catch_up,
        }
    }

    /// Adds real elapsed time and returns the whole steps now due.
    pub fn push(&mut self, real: Duration) -> Steps {
        let scaled = self.scaled_remainder + real.as_nanos() * u128::from(self.rate.hz());
        self.scaled_remainder = scaled % NANOS_PER_SEC;
        let due = scaled / NANOS_PER_SEC;
        let max = u128::from(self.max_catch_up.get());
        Steps {
            steps: due.min(max) as u32,
            dropped: u32::try_from(due.saturating_sub(max)).unwrap_or(u32::MAX),
        }
    }

    /// Real time accumulated toward the next step (rounded down to whole ns).
    pub fn remainder(&self) -> Duration {
        let nanos = self.scaled_remainder / u128::from(self.rate.hz());
        Duration::from_nanos(nanos as u64)
    }

    /// The fixed rate.
    pub fn rate(&self) -> TickRate {
        self.rate
    }
}
