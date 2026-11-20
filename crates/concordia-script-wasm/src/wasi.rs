//! WASI virtualized to the simulation: clocks read simulated time, randomness comes from the
//! processor's stream, and there is no filesystem, environment or stdin; stdout and stderr
//! become log lines.

use std::time::Duration;

use concordia_core::rng::SimRng;
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{HostMonotonicClock, HostWallClock, WasiCtx};

/// Most bytes of stdout or stderr kept per run.
const PIPE_CAPACITY: usize = 64 << 10;

/// Simulated seconds as whole nanoseconds (what the guest's clocks report); negative or
/// non-finite times read as zero.
pub fn sim_nanos(time: f64) -> u64 {
    if time.is_finite() && time > 0.0 {
        (time * 1e9).round() as u64
    } else {
        0
    }
}

/// A clock frozen at the run's simulated time (both wall and monotonic).
#[derive(Debug, Clone, Copy)]
struct SimClock(u64);

impl HostWallClock for SimClock {
    fn resolution(&self) -> Duration {
        Duration::from_nanos(1)
    }

    fn now(&self) -> Duration {
        Duration::from_nanos(self.0)
    }
}

impl HostMonotonicClock for SimClock {
    fn resolution(&self) -> u64 {
        1
    }

    fn now(&self) -> u64 {
        self.0
    }
}

/// A run's WASI context and its captured stdout and stderr.
pub(crate) struct RunWasi {
    pub(crate) ctx: WasiCtx,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
}

impl RunWasi {
    /// A closed context at simulated `time`, with randomness from `seed`'s streams 1 and 2.
    pub(crate) fn new(time: f64, seed: u64) -> Self {
        let clock = SimClock(sim_nanos(time));
        let stdout = MemoryOutputPipe::new(PIPE_CAPACITY);
        let stderr = MemoryOutputPipe::new(PIPE_CAPACITY);
        let rng = SimRng::new(seed);
        let ctx = WasiCtx::builder()
            .wall_clock(clock)
            .monotonic_clock(clock)
            .secure_random(rng.fork(1))
            .insecure_random(rng.fork(2))
            .insecure_random_seed(u128::from(seed))
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .build();
        Self {
            ctx,
            stdout,
            stderr,
        }
    }

    /// Captured output as log lines: stdout, then stderr.
    pub(crate) fn lines(&self) -> Vec<String> {
        [&self.stdout, &self.stderr]
            .into_iter()
            .flat_map(|pipe| {
                String::from_utf8_lossy(&pipe.contents())
                    .lines()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
