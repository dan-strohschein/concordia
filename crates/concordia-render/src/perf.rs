//! Per-pass GPU timing (M4 phase 4.1a): `PassTimer` wraps render-graph passes in RHI timer scopes,
//! and the frame's scopes become named pass times in `render.stats`.

use concordia_reflect::Reflect;
use concordia_rhi::{Command, CommandList, GpuTiming, TimerScope};
use serde::{Deserialize, Serialize};

/// Whether a frame's passes were timed.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum PassTiming {
    /// Perf mode is off (normal frames are not split per pass).
    #[default]
    off,
    /// Perf mode is on but the device cannot time passes (or they are masked).
    unsupported,
    /// Each pass was timed.
    measured,
}

/// One pass's GPU time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PassTime {
    /// The render-graph pass (or `lights.cluster`).
    pub name: String,
    /// When it started, in ms from the frame's first timed pass.
    pub start_ms: f64,
    /// Its GPU time (ms).
    pub gpu_ms: f64,
}

/// Wraps passes in timer scopes and remembers their names.
#[derive(Debug, Default)]
pub struct PassTimer {
    names: Vec<String>,
}

impl PassTimer {
    /// A timer with no passes yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a scope for pass `name` in `commands`.
    pub fn begin(&mut self, commands: &mut CommandList, name: &str) {
        let scope = TimerScope(self.names.len() as u32);
        commands.push(Command::BeginTimer(scope));
        self.names.push(name.to_owned());
    }

    /// Closes the open scope.
    pub fn end(&self, commands: &mut CommandList) {
        commands.push(Command::EndTimer);
    }

    /// The timed passes' names, in scope order.
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// Names `timing`'s scopes with `names` (scope `i` is pass `names[i]`).
pub fn pass_times(names: &[String], timing: &GpuTiming) -> Vec<PassTime> {
    timing
        .scopes
        .iter()
        .filter_map(|s| {
            Some(PassTime {
                name: names.get(s.scope.0 as usize)?.clone(),
                start_ms: s.start_ms,
                gpu_ms: s.ms(),
            })
        })
        .collect()
}
