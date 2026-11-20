# concordia-terra benchmark baseline

Recorded 2026-11-17 (M0 phase 0.3) on the reference machine (Apple M3 Pro, 18 GB), Rust 1.100.0, release profile.
Command: `just bench -p concordia-terra --bench iter -- --warm-up-time 1 --measurement-time 3`. Median of criterion's estimate.

| Benchmark | Time |
|---|---|
| `spawn_10k_two_components` | 3.16 ms |
| `iter_100k_read_one` | 69.9 µs |
| `iter_100k_write_two` | 43.9 µs |
| `par_iter_1m_write_two_1_threads` | 750 µs |
| `par_iter_1m_write_two_4_threads` | 223 µs |
| `par_iter_1m_write_two_8_threads` | 167 µs |
| `add_remove_component_10k` (20k archetype moves) | 2.15 ms |
| `world_hash_10k` | 4.37 ms |

Perf smoke (`just perf-smoke`): 1M-entity integrate step, median of 20, budget 25 ms; passes with large headroom.

## M2 phase 2.0b (2026-11-19)
Same machine and command, quiet (load average under 3). "Before" is main with phase 2.0 (`fcbcd91`); "after" is phase 2.0b.

| Benchmark | M0 baseline | Before 2.0b | After 2.0b |
|---|---|---|---|
| `spawn_10k_two_components` | 3.16 ms | 4.14 ms | 4.22 ms |
| `iter_100k_read_one` | 69.9 µs | 69.7 µs | 70.5 µs |
| `iter_100k_write_two` | 43.9 µs | 3.31 ms | 41.3 µs |
| `par_iter_1m_write_two_1_threads` | 750 µs | 37.8 ms | 749 µs |
| `par_iter_1m_write_two_4_threads` | 223 µs | 36.9 ms | 222 µs |
| `par_iter_1m_write_two_8_threads` | 167 µs | 36.7 ms | 166 µs |
| `add_remove_component_10k` | 2.15 ms | 2.67 ms | 2.97 ms |
| `world_hash_10k` | 4.37 ms | 4.43 ms | 4.44 ms |

Perf smoke: failed before (36.7 ms, budget 25 ms); passes after (3 of 3 runs).
- The write-query collapse came from M1 1.9e's change tracking (a `BTreeMap` lookup per visited entity). 2.0b keeps one change tick per archetype instead.
- `add_remove_component_10k` uses a zero-sized component, which is now a tag (ADR-0069); one extra lookup per insert is the remaining 11%.
- `spawn_10k` is still 34% above M0: every spawn and insert touches the entity through the id map (M1 1.9e). It's left as is.
