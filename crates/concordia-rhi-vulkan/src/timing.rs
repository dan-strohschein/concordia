//! Timer scopes (ADR-0089) on timestamp queries: query 0 opens the list, each scope takes two
//! queries in command order, and the last query closes the list. The tick math is pure.

use concordia_rhi::{Command, GpuTiming, ScopeTime, TimerScope};

/// Which queries a timed submission writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Queries {
    /// Each scope with its begin and end query.
    pub scopes: Vec<(TimerScope, u32, u32)>,
    /// Total queries (the first and last bracket the list).
    pub count: u32,
}

impl Queries {
    /// The queries for `commands`, with scopes only if the device times passes.
    pub fn plan(commands: &[Command], scopes: bool) -> Self {
        let mut planned = Vec::new();
        let mut next = 1;
        if scopes {
            for command in commands {
                if let Command::BeginTimer(scope) = command {
                    planned.push((*scope, next, next + 1));
                    next += 2;
                }
            }
        }
        Self {
            scopes: planned,
            count: next + 1,
        }
    }

    /// The query `BeginTimer(scope)` writes, if timed.
    pub fn begin(&self, scope: TimerScope) -> Option<u32> {
        self.scopes.iter().find(|s| s.0 == scope).map(|s| s.1)
    }

    /// The query the `EndTimer` closing `scope` writes, if timed.
    pub fn end(&self, scope: TimerScope) -> Option<u32> {
        self.scopes.iter().find(|s| s.0 == scope).map(|s| s.2)
    }

    /// The timing from each query's ticks (`None`: not available), `period_ns` per tick and the
    /// queue's `valid_bits`.
    pub fn timing(&self, ticks: &[Option<u64>], period_ns: f32, valid_bits: u32) -> GpuTiming {
        let mask = match valid_bits {
            0 => return GpuTiming::default(),
            64.. => u64::MAX,
            bits => (1u64 << bits) - 1,
        };
        let tick = |q: u32| ticks.get(q as usize).copied().flatten();
        let Some(first) = tick(0) else {
            return GpuTiming::default();
        };
        let ms = |t: u64| (t.wrapping_sub(first) & mask) as f64 * f64::from(period_ns) / 1e6;
        let scopes = self
            .scopes
            .iter()
            .filter_map(|(scope, b, e)| {
                let (start, end) = (ms(tick(*b)?), ms(tick(*e)?));
                Some(ScopeTime {
                    scope: *scope,
                    start_ms: start,
                    end_ms: end.max(start),
                })
            })
            .collect();
        GpuTiming {
            total_ms: tick(self.count - 1).map(ms),
            scopes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timed() -> Vec<Command> {
        vec![
            Command::BeginTimer(TimerScope(7)),
            Command::Dispatch { groups: [1, 1, 1] },
            Command::EndTimer,
            Command::BeginTimer(TimerScope(3)),
            Command::EndTimer,
        ]
    }

    #[test]
    fn scopes_take_two_queries_in_command_order() {
        let q = Queries::plan(&timed(), true);
        assert_eq!(q.scopes, [(TimerScope(7), 1, 2), (TimerScope(3), 3, 4)]);
        assert_eq!(q.count, 6);
        assert_eq!(
            (q.begin(TimerScope(3)), q.end(TimerScope(3))),
            (Some(3), Some(4))
        );
    }

    #[test]
    fn without_pass_timing_only_the_total_is_queried() {
        let q = Queries::plan(&timed(), false);
        assert!(q.scopes.is_empty());
        assert_eq!(q.count, 2);
        assert_eq!(q.begin(TimerScope(7)), None);
    }

    #[test]
    fn ticks_become_ms_from_the_first_sample() {
        let q = Queries::plan(&timed(), true);
        let ticks = [100, 200, 1_200, 1_300, 1_300, 2_100].map(Some);
        let timing = q.timing(&ticks, 1_000.0, 64);
        assert_eq!(timing.total_ms, Some(2.0));
        assert_eq!(
            timing.scopes[0],
            ScopeTime {
                scope: TimerScope(7),
                start_ms: 0.1,
                end_ms: 1.1
            }
        );
        assert_eq!(timing.scopes[1].ms(), 0.0);
    }

    #[test]
    fn wrapped_counters_and_missing_samples() {
        let q = Queries::plan(&timed(), true);
        let ticks = [
            Some(0xff0),
            Some(0xff8),
            Some(0x008),
            None,
            Some(0x010),
            Some(0x020),
        ];
        let timing = q.timing(&ticks, 1e6, 12);
        assert_eq!(timing.scopes.len(), 1, "scope 3 lacks its begin");
        assert_eq!(
            timing.scopes[0].ms(),
            16.0,
            "0xff8 → 0x008 wraps at 12 bits"
        );
        assert_eq!(timing.total_ms, Some(48.0));
        assert_eq!(
            q.timing(&ticks, 1.0, 0),
            GpuTiming::default(),
            "no timestamp bits"
        );
    }
}
