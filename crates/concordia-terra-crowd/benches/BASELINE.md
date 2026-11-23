# concordia-terra-crowd benchmark baseline

Recorded 2026-11-22 (M2 phase 2.5c.2) on the reference machine (Apple M3 Pro, 18 GB), release profile, load average about 6 (not quiet; treat as an upper bound).
Command: `cargo bench -p concordia-terra-crowd --bench crowd -- --warm-up-time 1 --measurement-time 3`. Median of criterion's estimate. **No pass/fail line:** `crowd_of_10k_agents_avoids_overlap` (on the reference machine, with M7's 100k target) is M7 7.7's.

`crowd_10k`: one tick (`nav.plan` and the whole crowd chain) of 2.5c.1's 10,000-agent field: surface agents on a 100 × 100 lattice 1.5 m apart, each walking at 3 m/s to the point opposite the centre. All of them are planned and walking before timing starts (160 ticks at 64 plans a tick). "LOD" puts a viewer at one corner with the `Crowd` template example's profile (20, 60, 120 m; rates 2, 4, 8). The far corner is 210 m away, so most agents are levels 2 and 3.

| Benchmark | 1 thread | 8 threads |
|---|---|---|
| `plain` (avoidance off, no LOD) | 30.5 ms | 12.3 ms |
| `lod` (avoidance off) | 29.6 ms | 11.3 ms |
| `avoidance` (no LOD) | 38.7 ms | 14.9 ms |
| `avoidance_lod` | 31.9 ms | 12.4 ms |

- **Avoidance** costs about 8 ms a tick on one thread (75 samples an agent, against at most 6 circles and 8 walls). LOD takes back about 7 ms of it, because far agents sample only when due.
- **LOD saves little without avoidance.** It only skips `crowd.corridor`, `crowd.steer` and `crowd.avoid`. The full-rate steps (grid, neighbours, collision, movement) are most of what is left: at 8 threads they are about 1.3, 1.5, 2.2 and 0.6 ms, measured with temporary timers.
- **Fixed while measuring.** The first run gave 27.4 ms (`plain`) and 29.2 ms (`avoidance`) at 8 threads, barely better than one thread. Two causes:
  - every Detour call went through one mutex-guarded pool of query objects, so 8 threads queued on it, and `crowd.corridor` and `crowd.move` were slower at 8 threads than at one. `NavMesh` now shards its pool 16 ways by thread. Results can't change: every Detour call resets the object it is given.
  - `crowd.collide` rebuilt a hash map of candidates each iteration. It now fills a vector indexed by grid slot.
- **Follow-ups for M7 7.7:** the sequential parts are `crowd.grid`'s build (hash maps and a sort by id), the collision snapshots and `nav.plan`'s scan of every agent. They are about 4 ms a tick at 10k, so a 100k crowd needs them parallel or incremental.
