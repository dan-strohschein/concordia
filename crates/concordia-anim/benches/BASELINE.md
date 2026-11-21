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

- The ECS adds about 6% over raw sampling, and eight threads give about 4.2× (64-entity chunks).
- Hashing poses is the expensive part: about 0.66 µs per bone transform, against about 0.44 µs per small component in `concordia-terra`'s `world_hash_10k`. Replays hash every 60 ticks by default, so this costs under 1 ms per tick on average. A faster hash for float arrays is a follow-up if crowds of full skeletons need frequent checkpoints.

**2.4.3 (2026-11-21, same machine, load average about 17: upper bounds).** The rows above were re-measured the same day and are unchanged within 1%. A graph tick samples three clips per character and costs 3.3× a one-clip tick (9.30 against 2.78 ms on one thread), so cost follows the clips sampled. Blending and per-node scratch poses add about 15% over three bare samples (3 × 2.63 ms). Eight threads give 4.5×. Graph state adds a few numbers per character, so the pose hash (above) is still the expensive part; its follow-up stays open.
