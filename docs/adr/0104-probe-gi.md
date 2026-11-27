# ADR-0104: Dynamic diffuse GI from probes
- Status: Accepted (M4 4.8, 2026-11-27; the 4.8 plan's G1–G16 and D24–D34, decided under the author's M4 rule)
- Date: 2026-11-27
- Builds on: ADR-0040 (RHI; amended for acceleration structures), ADR-0090 (quality methodology), ADR-0091 (shadows), ADR-0101 (the path tracer), ADR-0102 (emission in nits)

## Context
Light in M1–M4.7 stops at its first surface: a red wall doesn't tint the floor, a room lit through a door stays black, a fireball doesn't brighten the walls around it. The isometric ARPG camera sees a bounded slab of world with many fast-changing spell lights, so GI must cost per probe rather than per light and react within a few frames.

## Decision
1. **Technique:** DDGI-style probes (Majercik et al. 2019, 2021). `[render] gi = "off" | "probes"` (off by default), `[render.probes]` (rays per probe, probe counts, spacing, an optional fixed `center`), and `render.set_gi` for a session. A camera-following lattice (32 × 8 × 32 at 1.5 m by default, centred where the view meets the ground) or a fixed volume.
2. **Rays:** 256 per probe per frame (spherical Fibonacci, rotated per frame) through GI's own scene: proxy meshes simplified to 1% of their extent (`meshopt_simplify`), one look per material (diffuse albedo, emission in nits). Software BVH (`concordia-bvh`, shared with the path tracer) everywhere; hardware ray queries where the device has ray tracing (Metal now; Vulkan at M4's end), matched against the software path (FLIP 0.015).
3. **Hits:** the sun and every light in range with shadow rays (for the sun and for shadowed lights), emission, and last frame's probes (more bounces). Probes store physical irradiance; the forward pass multiplies by exposure.
4. **Atlases:** irradiance 8 × 8 and distance moments 16 × 16 per probe, octahedral, bordered for bilinear, in bindless buffers. Irradiance hysteresis adapts on each probe's mean luminance (0.97 down to 0.3, at most 0.5 on a light event); distance moments blend steadily. Chebyshev visibility, facing weights, no weight crushing (it caused flicker).
5. **Relocation and states:** while a lattice settles probes move out of geometry (at most 0.45 spacing) and go inactive when buried or seeing nothing. The lattice **scrolls** (toroidal slots): cells that enter start over, the rest keep what they learned. A new lattice warms up (40 updates) before its first frame.
6. **Sampling:** once per pixel in the forward pass, in place of the environment's SH9 diffuse; scenes without an environment get bounce light too.

## Consequences
- Against 8-bounce path-traced references: the iso arena (32 spell lights) 0.047 with the defaults (no GI 0.086); a 5 m room 0.082 with a room-sized volume (the default lattice is too coarse for it: 0.166). A thin wall keeps a dark room under 2% of its lit neighbour; a still scene doesn't flicker; a new light reaches 90% in four updates; no energy is gained.
- **Cost is far over M4's budget:** the canyon's update is 15.6 ms plus 2.8 ms of sampling on an M3 Pro's hardware ray queries, against 2.0 ms. A round-robin of 4 reached 4.9 ms but leaked through thin walls (2.25%), so it is off. GI is on by default only in `m4-lighting`; the canyon and the gate scenes keep it off until the budget work (`later-backlog.md`).
- Not done (backlog): a second, finer cascade near the camera (small rooms without a hand-placed volume), the stage B screen-space cache and its `gi::Technique` scaffold, texture means in material looks, the proxies' DDC cache, relocation after geometry moves.

## Alternatives considered
- **Screen-space GI only:** fails exactly for the isometric camera (emitters and occluders just off screen, under roofs). Kept as stage B's start, not built.
- **Light-propagation volumes / voxel cone tracing:** coarse and leak-prone at ARPG scales; probes test better against the path tracer and run on every backend.
- **Per-light virtual point lights:** cost grows with spell count.
