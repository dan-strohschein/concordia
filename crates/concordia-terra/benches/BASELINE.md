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

## M2 phase 2.4b.1: simulation LOD (2026-11-21)
Same machine, release, load average 3–6. Command: `cargo bench -p concordia-terra --bench lod -- --warm-up-time 1 --measurement-time 3`. One tick of a processor integrating `Pos += Vel × dt` over a 1 m grid, with one viewer at its center and the profile (20, 60, 120 m; rates 2, 4, 8; hysteresis 3 m). "Before" is that processor at full rate with no profiles and no `terra.lod` (what main runs); "after" is LOD-aware, with `terra.lod` first in `PrePhysics`.

| Benchmark | 1 thread | 8 threads |
|---|---|---|
| `full_tick_1k` (before) | 9.6 µs | 19.4 µs |
| `lod_tick_1k` (after) | 60.4 µs | 80.0 µs |
| `terra_lod_1k` (`terra.lod` alone) | 53.4 µs | 63.1 µs |
| `full_tick_100k` (before) | 43.3 µs | 42.7 µs |
| `lod_tick_100k` (after) | 6.74 ms | 2.04 ms |
| `terra_lod_100k` (`terra.lod` alone) | 6.55 ms | 2.02 ms |

- `terra.lod` costs about 65 ns per profiled entity on one thread (3.2× faster on eight). For a processor this cheap, LOD costs far more than it saves; it pays where per-entity work is heavy, as in animation (`concordia-anim`'s `BASELINE.md`: 4× fewer milliseconds at 100k characters).
- About 75% of it is position lookups: with a constant position, `terra_lod_100k_1_threads` is 1.67 ms. Each entity's position comes through `LodPosition` (a per-id `World::get`, since world positions follow the hierarchy). **Follow-up:** a position path in query order (roots read in the same pass) for M7's scale.
- The view change (due filtering through one row visitor) doesn't move the existing rows: `concordia-anim`'s `tick_1000_characters` is 2.73 / 0.68 ms against 2.59 / 0.68 ms before (within this run's noise).
