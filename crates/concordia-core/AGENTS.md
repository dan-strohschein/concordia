# concordia-core: agent guide

## Purpose
The root of the dependency graph: typed UUID ids and generators, glam math re-exports, fixed-step time, stderr-only logging with an in-memory ring for `log.tail`, a deterministic job system and a seeded simulation RNG.

## Public entry points
- `id::{EntityId, SceneId, TxnId, SessionId, PrefabId, AssetId}`, `id::IdSource`, `id::SystemIdGen`, `id::DeterministicIdGen`
- `time::{TickRate, Tick, Accumulator, Steps}`
- `jobs::JobSystem` (`par_chunks_mut`, `par_map_reduce`)
- `rng::SimRng` (`new`, `fork`, `seed`/`stream`/`word_pos` for state hashing)
- `suggest::did_you_mean` (Jaro-Winkler ≥ 0.8, top 3; used by scene and API errors)
- `log::{init, LogConfig, LogRing, LogRecord}`
- `math::{Vec2, Vec3, Vec4, Quat, Mat4, Affine3A}`, `math::Ray` (validated, unit direction; `intersect_plane`), `math::camera` (glam view/projection constructors; Metal uses `rh::proj::directx`)

## Invariants
- No dependencies on other Concordia crates.
- Generated ids are UUIDv7; parsing accepts v7 (engine) and v5 (imported content, `plans/README.md`) and rejects everything else.
- Sim code never reads wall-clock time or OS randomness: use `SimRng` and an `IdSource` (`DeterministicIdGen` in tests and sims).
- Logs go to stderr only; stdout belongs to stdio transports.
- Job chunk sizes are chosen by the caller (`NonZeroUsize`) and never depend on thread count; `par_map_reduce` combines in chunk-index order. Results are bit-identical for any thread count.
- The job system uses its own rayon pool, never the global one.
- `Accumulator` uses exact integer arithmetic (remainder in `ns × hz`), so it never drifts.

## How to test
`cargo nextest run -p concordia-core` and `cargo test --doc -p concordia-core`

## Common tasks
- **Add an id type:** add a doc-commented name to the `define_id!` list in `src/id.rs`; extend `tests/ids.rs` if it has special rules.
- **Add parallel work:** call `JobSystem::par_chunks_mut` with a constant chunk size; add a test comparing 1 vs 8 threads.
- **Add a random draw in a system:** take a `SimRng` (or `fork(stream)` one per system); never `rand::thread_rng`.

## Don'ts
- Don't print to stdout (`println!`) anywhere in engine crates.
- Don't derive a chunk size from `threads()`.
- Don't use `SystemIdGen` inside simulation code.
- Don't call `log::init` from libraries; binaries call it once.

## References
- RFC 9562 (UUIDv7 layout; deterministic ids put a 12-bit counter in `rand_a`).
- Steele, Lea & Flood, "Fast Splittable Pseudorandom Number Generators" (2014): SplitMix64 mixing for deterministic id bits.
- Glenn Fiedler, "Fix Your Timestep!" (Gaffer on Games).
- D. J. Bernstein, ChaCha (via the `rand_chacha` crate).
- rayon and tracing crate documentation.

## Status
M0 phase 0.1 complete.

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
