# concordia-anim benchmark baseline

Recorded 2026-11-21 (M2 phase 2.4.1) on the reference machine (Apple M3 Pro, 18 GB), release profile, load average about 9.5 (not quiet; treat as an upper bound).
Command: `cargo bench -p concordia-anim --bench sample -- --warm-up-time 1 --measurement-time 3`. Median of criterion's estimate.

A synthetic 65-bone chain; one 1 s clip at 30 fps with translation, rotation and scale keys on every bone (195 linear channels).

| Benchmark | Time |
|---|---|
| `sample_1000_characters_65_bones` (reset + sample, one thread, no ECS) | 2.64 ms |
| `tick_1000_characters_1_threads` (`anim.pose` + `anim.sample` through `SimLoop`) | 2.80 ms |
| `tick_1000_characters_8_threads` | 0.66 ms |
| `world_hash_1000_poses` (65,000 bone transforms) | 43.1 ms |
| `graph_1000_characters_1_threads` (2.4.3: a state machine over a `Blend2` of two clips under an active one-shot of a third, half-skeleton filter) | 9.30 ms |
| `graph_1000_characters_8_threads` | 2.08 ms |
| `events_attachments_1000_characters_1_threads` (2.4.4: clip players with four events a second, one item attached to the last bone of the 65-bone chain each) | 3.88 ms |
| `events_attachments_1000_characters_8_threads` | 1.90 ms |

- The ECS adds about 6% over raw sampling, and eight threads give about 4.2× (64-entity chunks).
- Hashing poses is the expensive part: about 0.66 µs per bone transform, against about 0.44 µs per small component in `concordia-terra`'s `world_hash_10k`. Replays hash every 60 ticks by default, so this costs under 1 ms per tick on average. A faster hash for float arrays is a follow-up if crowds of full skeletons need frequent checkpoints.

**2.4.3 (2026-11-21, same machine, load average about 17: upper bounds).** The rows above were re-measured the same day and are unchanged within 1%. A graph tick samples three clips per character and costs 3.3× a one-clip tick (9.30 against 2.78 ms on one thread), so cost follows the clips sampled. Blending and per-node scratch poses add about 15% over three bare samples (3 × 2.63 ms). Eight threads give 4.5×. Graph state adds a few numbers per character, so the pose hash (above) is still the expensive part; its follow-up stays open.

**2.4.4 (2026-11-21, same machine, load average about 11: upper bounds).** The earlier rows, re-measured the same run: 2.50, 2.59, 0.68, 42.3, 8.58 and 2.14 ms (within noise of the table). Events and attachments add about 1.3 ms per tick for 1,000 characters at one thread and 1.2 ms at eight: the attachment pass is sequential and walks the bone's chain, here the worst case (64 parents deep; real rigs are about 10). About 67 events are raised per tick. The pose-hash follow-up stays open: events don't need more frequent checkpoints.

**2.4b.1 (2026-11-21, same machine, load average 4–8: upper bounds): simulation LOD.** `lod_ticks` in `benches/sample.rs`: characters on a 2 m grid with a viewer at its center, the profile (20, 60, 120 m; rates 2, 4, 8). "Full" has no profiles (every character every tick, what main does, with `terra.lod` finding nothing); "LOD" gives each the profile, so `anim.sample` poses only due characters.

| Benchmark | 1 thread | 8 threads |
|---|---|---|
| `full_tick_1k_characters` (before) | 2.72 ms | 0.69 ms |
| `lod_tick_1k_characters` (after) | 1.90 ms | 0.60 ms |
| `full_tick_100k_characters` (before) | 280 ms | 52.2 ms |
| `lod_tick_100k_characters` (after) | 69.7 ms | 22.9 ms |

- At 100k, about 15% of characters pose each tick (314 at level 0, about 2,500 at 1, 8,500 at 2 and 88,700 at 3), so the sampling work falls to about 42 ms. `terra.lod`'s position lookups (`Transform` and `Parent` per character, about 7 ms) are most of the rest; see `concordia-terra`'s `BASELINE.md` for the follow-up.
- The earlier rows, re-measured in the same run: 2.64, 2.73, 0.68, 42.6, 9.12, 2.11, 4.02 and 1.92 ms (within noise of the table).
- **The 43 ms pose hash is unchanged by LOD**: poses held between due ticks are still hashed whenever a checkpoint is taken, and hashing cost follows the number of poses, not how often they change. LOD doesn't make it worse or better; the follow-up stays open.
