# concordia-ecs benchmark baseline

Recorded 2026-11-17 (M0 phase 0.3) on the reference machine (Apple M3 Pro, 18 GB), Rust 1.100.0, release profile.
Command: `just bench -p concordia-ecs --bench iter -- --warm-up-time 1 --measurement-time 3`. Median of criterion's estimate.

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
