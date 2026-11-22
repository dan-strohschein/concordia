# ADR-0076: Terra LOD: viewers, a hashed level, elapsed time, opt-in processors and signals
- Status: Accepted (M2 phase 2.4b.1, 2026-11-21; author decisions E1–E3)
- Date: 2026-11-21
- Builds on: ADR-0063 (determinism contract), ADR-0069 (storage kinds, commands, signals), ADR-0074 (animation runtime)

## Context
Mass-style crowds need far entities to cost less: fewer updates for what nobody is near. Concordia's rule (ADR-0063) is that the same inputs give the same world hash at any thread count, and replays (ADR-0072) re-run the simulation from its inputs. So simulation LOD can't depend on the render camera, on archetype layout or on thread count. It also needs memory (hysteresis) without hashing chunk data (ADR-0069 keeps chunk components unhashed). And a processor that skips an entity for a few ticks must still advance it by the right amount of time.

## Decision
1. **Viewers.** `LodViewer` (a tag) marks simulation entities that detail is measured from: the player, or a camera rig that is part of the scene. The render camera is never a viewer. With no viewer in the world, every entity is level 0, so headless tests and scenes without a player behave as before.
2. **Profiles.** `LodProfile { distances: [d1, d2, d3], rates: [r1, r2, r3], hysteresis }` is a shared component, so a template's `Lod` trait sets it once per archetype. Levels run from 0 (full) to 3; level 0 ticks every tick, level *i* every *rᵢ* ticks. Distances are finite, above 0 and ascending; rates are at least 1 and ascending; hysteresis is finite, at least 0 and below `d1`. An invalid profile in a world (one that bypassed `LodProfile::new`) means level 0. Entities without a profile are always level 0.
3. **The hashed level.** `SimLod { level, due, elapsed, last_time }` is a dense, hashed component, written only by `terra.lod`. That exclusive system runs first in `PrePhysics` each tick (the scene's core schedule installs it, with world-space positions). It gives profiled entities a `SimLod`, takes it from unprofiled ones, then for each:
   - **Level:** the nearest viewer by squared distance. A level is lost at a boundary (d ≥ dᵢ) and regained only inside it by the margin (d < dᵢ − hysteresis). Losing detail jumps straight to the new level, and so does gaining it.
   - **Due (E1, staggered ticks):** `(tick + phase(id)) mod rate = 0`, where `phase(id)` is SplitMix64 of the id's two 64-bit halves XORed (top 32 bits). An entity is also due on its first tick and on any tick its level changes.
   - **Elapsed:** when due, `elapsed = end − last_time` and `last_time = end`, where `end` is the clock's time plus this tick's scaled step (exactly what the clock reaches after the tick). It is exact under time-scale changes.
   - Only hashed inputs feed it (transforms, viewers, profiles, the clock and the id), so levels never depend on the camera, layout or threads.
4. **Opt-in processors.** `ProcessorDecl::lod()` makes a processor LOD-aware (it reads `SimLod`). All its queries visit only entities that are due this tick. The `*_due` variants (`for_each_due`, `par_for_each_due`, `par_for_each_deferred_due`) hand each entity a `Due { dt, dt_exact, level }`, its elapsed scaled time. Entities without a `SimLod` are due every tick with the tick's `dt`. Pieces are still cut every `chunk` rows, skipped rows included, so chunking never depends on LOD or thread count. Processors that don't opt in run every tick as before, and their `*_due` calls get the tick's `dt` at level 0.
5. **Signals.** Receivers see every signal and every signaled target, whatever its LOD: `View::signals` and `ViewQuery::for_each_of` ignore the due filter. LOD throttles only the regular query, so a signal is never cleared unseen.
6. **Animation is the first consumer (E3).** `anim.sample` opts in: far characters pose when due, advancing clips and graphs by their elapsed time. Events use 2.4.4's crossing rule on the whole step since the last pose, so each fires once, on the first due tick that covers it. Attachments and tweens stay full-rate (cheap, and a held pose places an attachment the same way every tick).

## Alternatives
- **Level in a chunk component.** Chunks aren't hashed (ADR-0069). Hysteresis would then make simulation depend on unhashed memory and on layout.
- **Level as a shared component** (archetype per level). Every transition would be a structural move.
- **Distances from the render camera.** Replays and headless runs would diverge from interactive play (M7 7.4's note).
- **Tick far entities in bulk every *r* ticks** (no stagger). Each *r*-th tick would do all the far work at once.
- **Ignoring level changes in the due rule.** Something walking into view could wait up to *r* − 1 ticks for its first fine update.
- **A per-processor rate table.** More knobs, and a processor's rate would have to agree with every other processor that touches the same entity.

## Consequences
- Play-world hashes change only for worlds with `LodProfile`s, which nothing had before. Edit worlds have no `SimLod` (it's added at play), so edit hashes and command journals are unchanged.
- `terra.stats` reports `lod` (viewers, entities per level, due this tick). `terra.lod` shows in `PrePhysics`'s waves.
- `concordia-terra` builds at `opt-level = 1` in dev, so its 100k-entity tests stay well inside nextest's hang guard.
- Representation LOD (full, instanced, hidden, chosen per frame from the drawing camera; E2) is 2.4b.2. It is presentation-only and never touches the hash.
- ADR-0069 expected LOD to be chunk components' first consumer. It isn't, so processor access to chunk components still waits for one.
