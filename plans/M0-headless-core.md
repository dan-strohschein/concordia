# M0: Headless Core
Status: **Gate passed (2026-11-18).** Layer 1 (`just gate M0`) green locally and in CI on macOS and Ubuntu (PR #1, merged as `75be979`); layer 2 (`just gate-live M0`) passed 2026-11-17 with Claude Code 2.1.327 (3 of 3 recorded sessions, then attempt 1 of 1 after the script fix); layer 3 (in-session check with the approved `concordia` project server, `/mcp` showing it connected) passed 2026-11-18 · Depends on: M-1 · Detail level: Ready to execute · Estimated effort: ~9–11 weeks (author + agents), 8 phases

## Goal & context

M0 builds the engine's headless heart and makes it drivable by an AI agent with no renderer and no API key. At the end of M0 an agent harness (Claude Code on a regular subscription) can create, edit, undo, save, reopen and query a scene entirely through MCP, and every change is a transaction attributed to an `Actor` and recorded in a persisted command journal.

This milestone sets the patterns every later milestone builds on:

- **Every type self-describes** (`concordia-reflect`): schema, doc strings, defaults and ranges. API and MCP tool schemas are *generated* from it.
- **Every mutation is a transaction** (`concordia-command`) with an `Actor`, undo/redo and a journal.
- **Text is the source of truth** (`concordia-scene`): canonical RON, stable UUIDv7 ids, one entity per block, byte-stable round-trips.
- **One API, many clients** (`concordia-api`): JSON-RPC 2.0 over stdio and WebSocket; MCP (`concordia-mcp`) and the CLI (`concordia-cli`) are thin adapters over it.
- **Determinism from day one**: fixed-step loop, seeded RNG, world-state hashing, identical across thread counts.

Design references: `docs/design.md` (principles 0–7, Architecture "Core", "World & Commands", "Engine API"), `plans/README.md` (shared conventions), ADR-0001, ADR-0002, ADR-0007, ADR-0008.

## Scope / Non-goals

**In scope**
- `concordia-core`: UUIDv7 ids (production and deterministic generators), glam re-export, fixed-step time, tracing setup (stderr only) and an in-memory log ring for `log.tail`, a rayon-backed job system with deterministic chunking, seeded `SimRng`.
- `concordia-reflect` + `concordia-reflect-derive`: type registry, `#[derive(Reflect)]`, doc-string capture, field attributes (default, range, skip), JSON Schema (2020-12) export, erased (de)serialization, canonical hashing.
- `concordia-ecs`: own archetype ECS (type-erased column storage, archetype graph, typed and dynamic queries, resources, sequential schedules with intra-system parallel iteration, fixed-step `SimLoop`, `WorldHash`), criterion benchmarks and a perf smoke test.
- `concordia-command`: `Actor`, `EditOp`, transactions (atomic, rollback on failure), linear undo/redo, persisted JSON-Lines journal, journal replay.
- `concordia-scene`: `.scene.ron` / `.prefab.ron` read/write, canonical formatter, stable ids, built-in components (`Name`, `Transform`, `Parent`, `Velocity`), proptest round-trips; **ADR-0009** (scene text format).
- `concordia-api`: typed method registry, JSON-RPC 2.0 dispatcher, stdio + WebSocket transports, sessions, edit world / play world, agent-actionable errors with suggestions.
- `concordia-mcp`: MCP server binary on `rmcp`, tools 1:1 with API methods, input/output schemas generated from `concordia-reflect`, MCP resources for schemas and docs.
- `concordia-cli`: `concordia new`, `concordia headless`, `concordia mcp`, `concordia schema` (+ proposed `concordia fmt`).
- Gate: automated MCP end-to-end test plus a scripted live-agent check with Claude Code headless.

**Non-goals (deferred)**
- Rendering, windowing, input, assets, physics (M1).
- Parallel *system* scheduling with access-conflict analysis (M0 runs systems sequentially in declared order; parallelism is inside systems). Revisit in M1 when render extraction needs it.
- Change detection / change ticks, observers, relations beyond `Parent`.
- Prefab *instance links* and per-instance overrides (M0 prefabs instantiate as flattened copies with fresh ids).
- Mutating the play world through the API (needs `input.inject`, M1); replay files (M3).
- Selective (per-actor) undo, merge of concurrent transactions, multi-scene worlds, streaming.
- Cross-platform floating-point determinism (M0 guarantees same machine + same binary; cross-platform is an M3 replay concern).
- Remote/attach mode for `concordia-mcp` (connecting to an already running `concordia headless`) — open question, likely M1 for the editor.

## Dependencies (what must already exist, from which milestone)

From **M-1** (verify each before Phase 0.1; if M-1 diverged, update this plan in the same change):
- Cargo workspace at the root with `[workspace.dependencies]`, `rust-toolchain.toml` pinned to a stable channel **≥ 1.88** (`rmcp` currently declares `rust-version = 1.88`; edition 2024 needs ≥ 1.85).
- `cargo-nextest`, `just`, clippy, rustfmt installed; `justfile` with `just test`, `just lint`, `just gate M<n>`.
- A cross-crate test package under `tests/` (assumed package name `concordia-tests`, `tests/Cargo.toml`; gate tests registered as `[[test]]` targets with `path = "gates/m0_*.rs"`).
- CI running fmt, clippy `-D warnings`, nextest, doc tests, `cargo-deny`.
- Claude Code CLI installed and logged in with the user's subscription (for the live gate only).

## Crates, modules & directories (created / touched)

```
crates/
  concordia-core/            src/{lib.rs, id.rs, time.rs, jobs.rs, rng.rs, log.rs, math.rs}
                          tests/{ids.rs, time.rs, jobs.rs, log_ring.rs}
  concordia-reflect/         src/{lib.rs, info.rs, registry.rs, dynamic.rs, schema.rs, hash.rs, impls/{primitives.rs, glam.rs, std.rs}}
                          tests/{derive.rs, schema.rs, schema_matches_serde.rs, registry.rs}
                          tests/ui/*.rs (trybuild compile-fail cases)
  concordia-reflect-derive/  src/{lib.rs, attrs.rs, expand_struct.rs, expand_enum.rs}
  concordia-ecs/             src/{lib.rs, entity.rs, storage/{blob_column.rs, archetype.rs, table.rs}, world.rs,
                               query/{mod.rs, data.rs, filter.rs, dynamic.rs, par.rs}, resource.rs,
                               schedule.rs, sim.rs, hash.rs, component.rs}
                          tests/{world.rs, queries.rs, model_proptest.rs, determinism.rs, perf_smoke.rs}
                          benches/iter.rs
  concordia-command/         src/{lib.rs, actor.rs, op.rs, txn.rs, log.rs, journal.rs, apply.rs}
                          tests/{actor.rs, txn.rs, undo_redo.rs, journal.rs, replay.rs}
  concordia-scene/           src/{lib.rs, model.rs, builtin.rs, canon/{ser.rs, fmt.rs}, read.rs, write.rs, prefab.rs}
                          tests/{roundtrip.rs, canonical.rs, proptest_roundtrip.rs, prefab.rs, errors.rs}
                          tests/snapshots/*.snap, tests/fixtures/*.scene.ron
  concordia-api/             src/{lib.rs, engine.rs, method.rs, methods/{scene.rs, entity.rs, component.rs, query.rs,
                               schema.rs, txn.rs, sim.rs, log.rs, session.rs}, error.rs, suggest.rs,
                               rpc.rs, session.rs, transport/{stdio.rs, ws.rs}}
                          tests/{dispatch.rs, errors.rs, sessions.rs, stdio.rs, ws.rs, sim.rs, method_schemas.rs}
  concordia-mcp/             src/{lib.rs, main.rs, tools.rs, resources.rs, docs/agent-guide.md}
                          tests/{tools.rs, resources.rs, stdout_hygiene.rs}
  concordia-cli/             src/{main.rs, cmd/{new.rs, headless.rs, mcp.rs, schema.rs, fmt.rs}, templates/}
                          tests/{cli_new.rs, cli_schema.rs, cli_headless.rs, cli_fmt.rs}
tests/
  gates/m0_headless_mcp.rs        # automated gate (layer 1)
  gates/m0_live_check.rs          # live-agent outcome checker (layer 2, #[ignore] in normal runs)
  gates/snapshots/*.snap
  src/{mcp_client.rs, bins.rs}       # helpers in the concordia-tests library
tools/
  gates/m0_live.sh                # runs `claude -p` against a fresh project, then the checker
  gates/m0_live_prompt.md         # the fixed prompt
  mcp/concordia-mcp.sh               # wrapper used by the repo-root .mcp.json
examples/m0-sandbox/              # tiny project: concordia.toml, scenes/main.scene.ron
docs/adr/0009-scene-text-format.md
docs/guides/agents-engine-api.md  # human/agent guide; also served as an MCP resource
.mcp.json                         # repo root: registers concordia-mcp for this Claude Code session
justfile                          # + gate-live, perf-smoke, bench recipes (no miri: ADR-0030)
.config/nextest.toml              # default-filter excludes m0_ gates from 0.7 until 0.8
```

### Workspace dependencies added (`[workspace.dependencies]`, verify versions at execution time)

| Crate | Version (2026-11-17 crates.io) | Used by | Notes |
|---|---|---|---|
| `uuid` | 1.29 | core | features `std`, `v7` (serde implemented on our id newtypes, which validate) |
| `glam` | 0.34 | core | features `serde` |
| `tracing` / `tracing-subscriber` | 0.1.44 / 0.3.23 | core, all | subscriber features `fmt`, `env-filter` |
| `rayon` | 1.12 | core | job system backend |
| `rand_chacha` / `rand_core` | 0.10 / matching | core | `SimRng`; check `rand_core` pairing at execution time |
| `serde` / `serde_json` | 1 / 1.0.154 | all | `serde_json` feature `preserve_order` **off** (we sort ourselves) |
| `erased-serde` | 0.4 | reflect | erased (de)serialization in registry |
| `syn` / `quote` / `proc-macro2` | 3.0 / 1 / 1 | reflect-derive | syn 3 chosen (verified 2026-11-17; MSRV 1.71) |
| `blake3` | 1.8 | reflect, ecs, command | hashing |
| `thiserror` | 2 | all | error enums |
| ~~`ron`~~ | — | scene | **dropped**: own type-directed reader/writer (ADR-0009) |
| `ryu` | 1 | scene | shortest round-trip float formatting |
| `smallvec`, `rustc-hash`, `indexmap` | 1.16 / 2.1 / 2.14 | ecs | |
| ~~`json-patch`~~ | — | api | **dropped**: RFC 7396 merge patch is ~20 lines in-house, tested against the RFC's appendix vectors |
| `strsim` | 0.11 | core (`suggest`), used by scene and api | did-you-mean suggestions |
| `tokio` | 1.54 | api, mcp, cli | `rt-multi-thread`, `macros`, `io-std`, `net`, `sync`, `time` |
| `tokio-tungstenite` | 0.30 | api | WebSocket transport |
| `futures` | 0.3 | api | |
| `rmcp` | `=3.11.0` (exact pin, ADR-0033) | mcp, tests | `default-features = false`, features `server`, `client`, `transport-io`, `transport-child-process`; `server` pulls `schemars` transitively (unused by us) |
| `clap` | 4.6 | cli, mcp | `derive` |
| dev: `insta` 1.49 (`json`, `redactions`), `proptest` 1.12, `criterion` 0.8, `trybuild` 1, `jsonschema` 0.61 (`default-features = false`: defaults pull reqwest + aws-lc-rs), `tempfile` 3, `assert_cmd` 2 | | tests | |

All are MIT and/or Apache-2.0 (ADR-0006); `cargo deny check licenses` must stay green. Versions re-verified against crates.io on 2026-11-17 (phase 0.0): no drift.

### Per-crate `AGENTS.md` content expectations

Every crate's `AGENTS.md` has these headings: **Purpose** (2–3 sentences), **Public entry points** (the 3–8 types/functions an agent should start from), **Invariants** (must never break), **How to test** (exact commands), **Common tasks** (step lists), **Don'ts**, **References** (clean-room provenance). Crate-specific content:

| Crate | Invariants / common tasks that must appear |
|---|---|
| `concordia-core` | generated ids are UUIDv7 (v5 accepted for imported content, per plans/README.md); sim code never reads wall-clock time or OS randomness (use `SimRng`, `IdSource`); logs go to stderr only; job chunk sizes never depend on thread count. |
| `concordia-reflect` | serde shape == reflected shape (forbidden serde attrs listed); every public reflected type and field has a doc comment (it becomes agent-facing schema text); task: "make a type reflectable", "add a range/default". |
| `concordia-reflect-derive` | supported shapes and attributes; how to add a trybuild case. |
| `concordia-ecs` | all `unsafe` in `storage/` with SAFETY comments; `WorldHash` independent of archetype layout and thread count; systems run in declared order; task: "add a component", "write a system", "add a determinism test". |
| `concordia-command` | the only path to mutate an edit world; inverses computed at apply time; journal is append-only JSONL with `v`; task: "add a new EditOp" (inverse + journal + replay test). |
| `concordia-scene` | canonical rules (list); byte-identical round-trip; entities sorted by id; task: "add a fixture", "bump the format version". |
| `concordia-api` | every mutating method takes an actor and goes through `CommandLog`; every error kind has a suggestion; method docs are agent-facing; task: "add a method" (params/result types, tests, snapshot update, MCP picks it up automatically). |
| `concordia-mcp` | no hand-written schemas; stdout is protocol-only; rmcp isolated in two files; task: "upgrade rmcp". |
| `concordia-cli` | subcommands are thin wrappers over `concordia-api`/`concordia-scene`; template snapshots. |

## Phases

### Phase 0.1: concordia-core

- **Objective:** foundational types every crate uses: ids, math, time, logging, jobs, deterministic RNG.
- **Deliverables:** `crates/concordia-core` with modules `id`, `math`, `time`, `log`, `jobs`, `rng`; `AGENTS.md`, `README.md`; doc tests on every public item.
- **Tests first**
  - `tests/ids.rs`
    - `system_id_generator_produces_uuid_v7` — `SystemIdGen::next()` returns a UUID whose `get_version_num() == 7`.
    - `system_ids_are_monotonic_within_process` — 10 000 consecutive ids are strictly increasing (`Ord` on bytes).
    - `deterministic_id_generator_is_reproducible` — two `DeterministicIdGen::new(seed=42)` produce identical sequences of 1 000 ids.
    - `deterministic_ids_are_valid_v7_and_sorted` — version 7, variant RFC 9562, strictly increasing.
    - `entity_id_roundtrips_through_string_and_serde` — `EntityId::from_str(id.to_string())` and serde_json round-trip; display is the hyphenated lowercase form.
    - `entity_id_rejects_non_v7_with_message` — parsing a v4 UUID returns `IdError::WrongVersion { found: 4 }`.
  - `tests/time.rs`
    - `accumulator_emits_whole_steps_and_keeps_remainder` — at 60 Hz, pushing 40 ms yields 2 steps and 6.67 ms remainder (tick-integer arithmetic in nanoseconds; no float drift).
    - `accumulator_clamps_catch_up` — pushing 10 s yields at most `max_catch_up` steps (default 8) and reports `dropped_steps`.
    - `fixed_clock_tick_to_seconds_is_exact` — `Tick(60).as_secs_f64(TickRate::HZ_60) == 1.0`.
  - `tests/jobs.rs`
    - `par_chunks_mut_visits_every_element_once` — 1 000 003 elements, each incremented exactly once, with 1, 2, 4, 8 threads.
    - `par_map_reduce_is_thread_count_invariant` — f32 sum of 1M pseudo-random values reduced via `par_map_reduce` is bit-identical for 1, 2, 3, 8 threads (fixed chunk boundaries, ordered combine).
    - `job_panic_propagates_to_caller` — panic in a job surfaces as a panic in `scope` (no deadlock).
  - `tests/log_ring.rs` (as built, also `log_ring_records_level_target_message_and_fields`; ids also gained `imported_v5_ids_are_accepted`, `malformed_id_text_is_a_parse_error`; time gained `accumulator_has_no_drift_over_many_pushes`)
    - `log_ring_keeps_last_n_records_with_monotonic_seq` — capacity 4, 10 events → seq 7..=10.
    - `log_ring_tail_since_returns_only_newer` — `tail(since: Some(8))` returns 9, 10.
    - `log_init_never_writes_to_stdout` — spawn a child test binary that inits logging and emits at all levels; its captured stdout is empty (critical: stdio transports own stdout).
  - Unit: `sim_rng_same_seed_same_stream`, `sim_rng_streams_split_deterministically` (`SimRng::fork(stream_id)`).
  - Added in phase 0.0: `deterministic_id_gen_exhaustion_is_an_error` (counter overflow → `IdError`, never wraps), `tick_rate_from_hz_rejects_zero` (`Result`). Chunk sizes and thread counts are `NonZeroUsize`, so zero is a compile error. `log_init_never_writes_to_stdout` re-executes its own test binary with an env var.
- **Implementation notes**
  ```rust
  // id.rs — all ids are UUIDv7 newtypes generated by a macro.
  define_id!(EntityId, SceneId, TxnId, SessionId, PrefabId, AssetId);
  pub trait IdSource { fn next_uuid(&mut self) -> Result<Uuid, IdError>; fn next_id<I: TypedId>(&mut self) -> Result<I, IdError>; }
  // As built (0.1): fallible because DeterministicIdGen can exhaust; ids validate on construction (v7, or v5 for imports).
  // Callers such as World::spawn therefore return Result.
  pub struct SystemIdGen;           // uuid::Uuid::now_v7() (uuid has an internal counter for monotonicity)
  pub struct DeterministicIdGen { seed: u64, counter: u64, base_ms: u64 }
  // DeterministicIdGen: uuid::Builder::from_unix_timestamp_millis(base_ms + counter / 4096, &rand_bytes(seed, counter))
  //   with 12-bit counter in rand_a so ordering is strict. Used by tests, `--deterministic-ids <seed>`, and sim.
  // time.rs
  pub struct TickRate(NonZeroU32); pub struct Tick(pub u64);
  pub struct Accumulator { rate: TickRate, remainder_ns: u64, max_catch_up: u32 }
  impl Accumulator { pub fn push(&mut self, real: Duration) -> Steps /* { steps: u32, dropped: u32 } */ }
  // jobs.rs — thin wrapper over a rayon ThreadPool we own (never the global pool).
  pub struct JobSystem { pool: rayon::ThreadPool }
  impl JobSystem {
      pub fn new(threads: NonZeroUsize) -> Self;
      pub fn par_chunks_mut<T: Send>(&self, data: &mut [T], chunk: NonZeroUsize, f: impl Fn(usize, &mut [T]) + Sync);
      pub fn par_map_reduce<T: Sync, A: Send>(&self, data: &[T], chunk: NonZeroUsize,
          map: impl Fn(&[T]) -> A + Sync, reduce: impl Fn(A, A) -> A) -> Option<A>; // ordered by chunk index
  }
  // log.rs
  pub fn init(cfg: &LogConfig) -> LogHandle; // fmt layer → stderr, EnvFilter (CONCORDIA_LOG), + LogRing layer
  pub struct LogRing { /* Mutex<VecDeque<LogRecord>>, next_seq */ }
  pub struct LogRecord { pub seq: u64, pub level: Level, pub target: String, pub message: String, pub fields: BTreeMap<String, String> }
  // rng.rs: pub struct SimRng(rand_chacha::ChaCha8Rng); seed + stream split via set_stream.
  // math.rs: pub use glam::{Vec2, Vec3, Vec4, Quat, Mat4, Affine3A};
  ```
  Determinism rule (documented in `AGENTS.md`): chunk size is a constant chosen by the caller, never derived from thread count; reductions combine in chunk-index order.
  References: RFC 9562 (UUIDv7); Glenn Fiedler, "Fix Your Timestep!" (Gaffer on Games); rayon docs; ChaCha (D. J. Bernstein) via `rand_chacha`.
- **Done criteria:** all tests above green; `cargo clippy -p concordia-core -- -D warnings` clean; every public item documented (workspace `missing_docs` lint, an error under clippy `-D warnings`); `AGENTS.md` written.

### Phase 0.2: concordia-reflect + concordia-reflect-derive

- **Objective:** every engine type can describe itself (shape, docs, defaults, ranges) and be (de)serialized, hashed and schema-exported dynamically by name, so agents can discover and validate anything.
- **Deliverables:** `TypeInfo` model, `Reflect` trait, `TypeRegistry` with type-data extension slots, `#[derive(Reflect)]`, JSON Schema 2020-12 exporter, `DynReflect` object-safe trait, canonical hasher; impls for primitives, `String`, `Option`, `Vec`, `BTreeMap<String, _>`, arrays, tuples (≤4), glam `Vec2/3/4`, `Quat`, `Mat4`, `serde_json::Value` (opaque "any"), all `concordia-core` ids.
- **Tests first**
  - `tests/derive.rs`
    - `derive_captures_struct_doc_and_field_docs` — `/// A health pool.` on the struct and `/// Current hit points.` on a field appear in `TypeInfo.docs` / `FieldInfo.docs` (trimmed, multi-line joined with `\n`).
    - `derive_supports_named_tuple_unit_structs_and_enums` — shapes `Struct`, `TupleStruct`, `Unit`, `Enum{unit, tuple, struct variants}` are produced correctly.
    - `derive_records_default_and_range_attrs` — `#[reflect(default = 100.0, range(min = 0.0, max = 1000.0))]` lands in `FieldInfo`.
    - `derive_name_defaults_to_ident_and_can_be_overridden` — `TypeInfo.name == "Health"`; `#[reflect(name = "game.Health")]` overrides.
    - `derive_transparent_newtype_reflects_inner` — `#[serde(transparent)] struct Name(String)` → `Shape::Newtype { transparent: true }`.
  - `tests/ui/*.rs` via `trybuild` (compile-fail with clear messages):
    - `generic_struct_rejected` — "Reflect does not support generic types in M0".
    - `serde_rename_rejected` — `#[serde(rename/rename_all/tag/untagged/flatten)]` → "serde renaming/tagging changes the wire shape; use #[reflect(name)] on the type instead" (keeps schema and serde in lockstep).
    - `union_rejected`.
    - `range_min_greater_than_max_rejected` (added in phase 0.0).
  - `tests/registry.rs`
    - `register_is_idempotent_and_registers_field_types` — registering `Transform` also registers `Vec3`, `Quat`.
    - `duplicate_stable_name_is_an_error` — two types named `Health` → `RegistryError::NameCollision { name, existing, new }`.
    - `lookup_by_name_and_type_id` — `get_by_name("Health")`, `get::<Health>()` agree.
    - `type_data_slots_roundtrip` — `insert_data::<ReflectFoo>` / `data::<ReflectFoo>()`.
    - `dyn_from_json_to_json_roundtrip` — `registration.from_json(json)` → `DynReflect` → `to_json()` equals input.
    - `dyn_from_json_reports_json_pointer_on_error` — `{"hp": "lots"}` → `ReflectError::Invalid { pointer: "/hp", expected: "number", found: "string" }`.
    - `from_json_rejects_unknown_type_name` (added in phase 0.0).
  - `tests/schema.rs`
    - `schema_snapshot_health` / `schema_snapshot_transform` / `schema_snapshot_enum_shape` — `insta::assert_json_snapshot!` of the generated schemas (docs → `description`, defaults → `default`, ranges → `minimum`/`maximum`, `x-concordia-type`).
    - `generated_schemas_validate_against_draft_2020_12_metaschema` — every registered type's schema is a valid JSON Schema (via `jsonschema`).
    - `registry_index_schema_lists_all_types_sorted` — stable ordering for snapshots and MCP resources.
  - `tests/schema_matches_serde.rs` (proptest)
    - `serialized_values_validate_against_their_schema` — for arbitrary values of a zoo of test types (nested structs, enums of all variant kinds, `Option`, `Vec`, maps, glam types, ids; finite floats), `serde_json::to_value(v)` validates against `schema_for::<T>()`.
    - `schema_rejects_wrong_shapes` — hand-picked invalid instances (missing required field, wrong enum variant, extra field when `deny_unknown_fields`) fail validation.
  - Unit (`hash.rs`): `canonical_hash_equal_for_equal_values`, `canonical_hash_distinguishes_neg_zero_and_field_order_invariant` (map key order irrelevant; `-0.0 != 0.0` bitwise), `canonical_hash_stable_snapshot` (hex digest of a fixed value is snapshotted to catch accidental format changes).
- **Implementation notes**
  ```rust
  pub trait Reflect: Serialize + DeserializeOwned + Clone + PartialEq + Send + Sync + 'static {
      fn type_info() -> &'static TypeInfo;                 // OnceLock in generated code
      fn register_dependencies(_reg: &mut TypeRegistry) {} // derive registers field types
  }
  pub struct TypeInfo { pub name: &'static str, pub rust_path: &'static str, pub docs: &'static str,
                        pub shape: Shape, pub attrs: TypeAttrs }
  pub enum Shape { Primitive(Primitive), Struct { fields: Vec<FieldInfo>, deny_unknown: bool },
      TupleStruct { fields: Vec<TypeRef> }, Newtype { inner: TypeRef, transparent: bool }, Unit,
      Enum { variants: Vec<VariantInfo> }, Option(TypeRef), List(TypeRef), Map { value: TypeRef },
      Array { elem: TypeRef, len: usize }, Tuple(Vec<TypeRef>), Any }
  pub struct FieldInfo { pub name: &'static str, pub ty: TypeRef, pub docs: &'static str,
      pub default: Option<fn() -> serde_json::Value>, pub range: Option<(f64, f64)>, pub skip: bool }
  pub struct TypeRef { pub name: &'static str, pub info: fn() -> &'static TypeInfo }

  pub trait DynReflect: Any + Send + Sync {            // object-safe
      fn info(&self) -> &'static TypeInfo;
      fn as_any(&self) -> &dyn Any; fn as_any_mut(&mut self) -> &mut dyn Any;
      fn clone_dyn(&self) -> Box<dyn DynReflect>;
      fn eq_dyn(&self, other: &dyn DynReflect) -> bool;
      fn as_serialize(&self) -> &dyn erased_serde::Serialize;
  }
  pub struct TypeRegistration { pub info: &'static TypeInfo, pub type_id: TypeId,
      pub from_json: fn(serde_json::Value) -> Result<Box<dyn DynReflect>, ReflectError>,
      pub deserialize: fn(&mut dyn erased_serde::Deserializer) -> Result<Box<dyn DynReflect>, erased_serde::Error>,
      pub default: Option<fn() -> Box<dyn DynReflect>>, data: TypeDataMap }
  impl TypeRegistry { pub fn register<T: Reflect>(&mut self) -> &mut TypeRegistration; pub fn get_by_name(&self, n: &str) -> Option<&TypeRegistration>;
      pub fn schema_for(&self, name: &str) -> Option<serde_json::Value>; pub fn schema_index(&self) -> serde_json::Value;
      pub fn names(&self) -> impl Iterator<Item = &'static str>; }
  pub fn canonical_hash(v: &dyn DynReflect, h: &mut blake3::Hasher); // custom serde Serializer emitting tagged bytes,
                                                                     // floats as to_bits(), struct/map keys sorted
  ```
  - Value transport is **serde** (one source of wire truth); reflection adds shape/docs. The derive forbids serde attributes that would make serde's JSON diverge from `TypeInfo`; a proptest proves they agree.
  - Schema export: `$schema: https://json-schema.org/draft/2020-12/schema`, `$defs` for referenced types, `title` = stable name, `description` = docs, `additionalProperties: false` for structs, enums as `oneOf` of externally-tagged forms (serde default), glam `Vec3` as `{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3,"description":"[x, y, z]"}`, ids as `{"type":"string","format":"uuid"}`.
  - Derive written with `syn`/`quote` from scratch; we read Bevy's public reflection *docs* for the problem space but write our own design (clean room is about Unreal/source-available engines; Bevy is MIT/Apache, but we still don't copy).
  - References: JSON Schema 2020-12 spec; serde data model docs; `erased-serde` docs; Rust Reference on proc macros and `#[doc]` attributes.
- **As built (0.2):**
  - `Reflect` has no `register_dependencies`: `TypeRef` carries `info` and `register` fn pointers, and the registry walks `Shape::refs()` (recursive types terminate because a type is marked before its fields are walked).
  - `TypeInfo` has a `structural` flag instead of `attrs`; structural types (`Option`, `Vec`, `BTreeMap<String,_>`, arrays, tuples) are inlined and not registered by name. Generic impls cache their info per `TypeId` (statics in generic fns are shared).
  - `FieldInfo` has `optional` (serde `default`, `skip_serializing_if`, or `Option`) instead of `skip`; `#[serde(skip)]` fields are omitted. There is no `#[reflect(skip)]` (it would let schema and serde diverge).
  - The derive uses a serde attribute **allow-list** (listed in `concordia-reflect/AGENTS.md`), not a deny-list.
  - `TypeRegistration::from_json` validates shape and declared ranges first (`ReflectError::{Invalid, MissingField, UnknownField, OutOfRange}` with JSON Pointers), then deserializes. Added `TypeRegistry::from_json(name, value)` (`UnknownType`).
  - Schemas: structs and enums go in `$defs`, the root is inlined, recursion to the root is `{"$ref": "#"}`; a field's docs are combined with an inlined type's docs.
  - `canonical_hash` returns `Result`; integers are widened to `i128`, type names are excluded, struct fields and map keys sorted.
  - Extra tests: `serde_skip_fields_are_not_reflected`, `deny_unknown_fields_is_recorded`, `dyn_from_json_reports_missing_unknown_and_out_of_range`, `erased_deserialize_builds_value`, `dyn_clone_and_eq`, `unknown_name_has_no_schema`, `canonical_hash_distinguishes_variants_and_one_ulp`, trybuild `serde_flatten_rejected`. Hash tests live in `tests/hash.rs`.
- **Done criteria:** all tests green including trybuild; schema snapshots reviewed; `cargo doc` shows examples; `AGENTS.md` documents "how to make a type reflectable" and the forbidden-serde-attrs rule.

### Phase 0.3: concordia-ecs

- **Objective:** our own archetype ECS with typed and dynamic access, a deterministic fixed-step loop and world-state hashing.
- **Deliverables:** `World`, `Entity` handles mapped to stable `EntityId`, archetype storage, typed queries with `With`/`Without` filters, parallel query iteration via `JobSystem`, resources, `Schedule`, `SimLoop`, `WorldHash`, dynamic (by-name) component access through `ReflectComponent` type data, `DynamicQuery`, `World::clone_with(&TypeRegistry)` (used for play worlds), criterion bench, perf smoke test.
- **Tests first**
  - `tests/world.rs`
    - `spawned_entity_has_stable_uuid` — `spawn_with_id(id)`; `world.entity(id)` resolves; despawn + respawn of *other* entities never changes it.
    - `spawn_with_duplicate_id_fails` — `EcsError::DuplicateId(id)`.
    - `insert_moves_entity_between_archetypes_preserving_other_components` — add `C` to an `(A, B)` entity; `A`, `B` values unchanged; archetype count == 2.
    - `remove_last_component_keeps_entity_alive` — entities may be empty (an id with no components).
    - `despawn_drops_components_exactly_once` — drop-counting component, counts match after despawn, archetype moves and `World` drop.
    - `stale_handle_detected_by_generation` — after despawn, the old `Entity` handle returns `None`.
    - `zero_sized_and_high_alignment_components_work` — ZST marker and `#[repr(align(64))]` type.
    - `resources_insert_get_replace_remove`.
  - `tests/queries.rs`
    - `query_yields_only_matching_archetypes` — `(&A, &mut B)` with `Without<C>`.
    - `query_mut_writes_are_visible` — integrate `Velocity` into `Transform`.
    - `query_state_picks_up_archetypes_created_later`.
    - `aliasing_mutable_access_is_rejected` — `(&mut A, &A)` → `Err(QueryError::Conflict)` from `QueryState::new` (public APIs never panic; so `World::query*` return `Result`).
    - `par_for_each_mut_matches_sequential_result` — 100k entities, 1 vs 8 threads, identical components.
    - `dynamic_query_by_names_returns_sorted_entity_ids` — `DynamicQuery { with: ["Transform"], without: ["Velocity"] }` yields ids sorted ascending.
    - `dynamic_get_set_json_roundtrip` — `world.set_json(id, "Transform", json, &reg)` then `get_json`.
  - `tests/model_proptest.rs` (proptest, stateful model test)
    - `world_matches_hashmap_model_under_random_ops` — random sequences of spawn/despawn/insert/remove/get over 3 component types; after each op the `World` matches a `BTreeMap<EntityId, (Option<A>, Option<B>, Option<C>)>` model; entity count and per-archetype column lengths agree (catches unsafe storage bugs).
  - `tests/determinism.rs`
    - `same_inputs_same_hash_after_1000_ticks` — two worlds built from the same seed run 1000 ticks of a schedule (velocity integration + `SimRng`-driven jitter system) → identical `WorldHash`.
    - `hash_identical_across_thread_counts` — same simulation with `JobSystem` of 1, 2, 4, 8 threads → identical hashes at ticks 1, 10, 100, 1000.
    - `hash_independent_of_spawn_and_archetype_history` — same final state reached via different insertion orders / add-remove detours → equal hash.
    - `hash_changes_when_any_component_bit_changes` — flip one f32 by 1 ulp → different hash.
    - `sim_loop_steps_exactly_n_ticks` — `SimLoop::step(n)` advances `SimClock.tick` by n regardless of real time.
  - `tests/perf_smoke.rs`
    - `perf_smoke_iterate_1m_transform_velocity` — `#[cfg_attr(debug_assertions, ignore = "release only: just perf-smoke")]`; 1M entities, integrate step median of 20 runs < **25 ms** single-threaded on the dev machine (generous ~10x headroom; guards against pathological regressions, not a benchmark).
  - `benches/iter.rs` (criterion): `spawn_10k_two_components`, `iter_100k_read_one`, `iter_100k_write_two`, `par_iter_1m_write_two_{1,4,8}_threads`, `add_remove_component_10k` (archetype moves), `world_hash_10k`.
- **Implementation notes**
  ```rust
  pub trait Component: Reflect {}                       // blanket impl for every Reflect type
  impl<T: Reflect> Component for T {}
  pub fn register_component<T: Component>(reg: &mut TypeRegistry); // adds ReflectComponent type data
  pub struct ReflectComponent {                        // erased ops used by commands/API/scene
      pub insert_dyn: fn(&mut World, EntityId, Box<dyn DynReflect>) -> Result<Option<Box<dyn DynReflect>>, EcsError>,
      pub get_dyn: fn(&World, EntityId) -> Option<&dyn DynReflect>,
      pub remove_dyn: fn(&mut World, EntityId) -> Result<Option<Box<dyn DynReflect>>, EcsError>,
  }
  pub struct Entity { index: u32, generation: NonZeroU32 }        // internal handle
  pub struct World { entities: EntityTable, ids: HashMap<EntityId, Entity>, archetypes: Archetypes,
                     components: ComponentIndex, resources: Resources }
  impl World {
      pub fn spawn_with_id(&mut self, id: EntityId) -> Result<EntityWorldMut<'_>, EcsError>;
      pub fn spawn(&mut self, ids: &mut impl IdSource) -> EntityWorldMut<'_>;
      pub fn despawn(&mut self, id: EntityId) -> Result<(), EcsError>;
      pub fn insert<C: Component>(&mut self, id: EntityId, c: C) -> Result<Option<C>, EcsError>;
      pub fn remove<C: Component>(&mut self, id: EntityId) -> Result<Option<C>, EcsError>;
      pub fn get<C: Component>(&self, id: EntityId) -> Option<&C>;
      pub fn get_mut<C: Component>(&mut self, id: EntityId) -> Option<&mut C>;
      pub fn query<D: QueryData>(&mut self) -> QueryState<D, ()>;
      pub fn query_filtered<D: QueryData, F: QueryFilter>(&mut self) -> QueryState<D, F>;
      pub fn entity_ids_sorted(&self) -> Vec<EntityId>;
      pub fn component_names(&self, id: EntityId, reg: &TypeRegistry) -> Vec<&'static str>;
      pub fn state_hash(&self, reg: &TypeRegistry) -> WorldHash;          // blake3 → [u8; 32]
      pub fn clone_with(&self, reg: &TypeRegistry) -> World;
  }
  pub struct Schedule { systems: Vec<BoxedSystem> }                       // runs in insertion order
  pub trait System: Send { fn name(&self) -> &str; fn run(&mut self, world: &mut World, ctx: &SystemCtx); }
  impl<F: FnMut(&mut World, &SystemCtx) + Send> System for NamedFn<F> { .. }
  pub struct SystemCtx<'a> { pub jobs: &'a JobSystem, pub tick: Tick, pub dt: f32 /* fixed */ }
  pub struct SimLoop { pub rate: TickRate, pub schedule: Schedule, acc: Accumulator }
  impl SimLoop { pub fn step(&mut self, world: &mut World, jobs: &JobSystem, n: u32);
                 pub fn advance(&mut self, world: &mut World, jobs: &JobSystem, real: Duration) -> Steps; }
  ```
  - Storage (superseded by ADR-0030, see As built): one `Table` per archetype; swap-remove on despawn with entity-location fix-up; archetype graph edges cache add/remove transitions.
  - `WorldHash`: iterate `entity_ids_sorted()`, for each id hash the id bytes then each component sorted by stable name, via `concordia_reflect::canonical_hash`; also hash `SimClock` and `SimRng` state. Independent of archetype layout and thread count by construction.
  - Parallelism: `QueryState::par_for_each_mut(&mut world, jobs, CHUNK = 4096, f)`; chunks are per-table row ranges, fixed size.
  - **ADR-0030** (own archetype ECS) is written in this phase.
  - References (published designs, no code copied): Sander Mertens, "Building an ECS" article series (flecs author); Michele Caini, "ECS back and forth" (EnTT author); Bevy ECS documentation and RFCs (design concepts only); Timothy Ford, "Overwatch Gameplay Architecture and Netcode" (GDC 2017); Richard Fabian, *Data-Oriented Design*; Fiedler, "Fix Your Timestep!" and "Floating Point Determinism".
- **As built (0.3):**
  - **Safe storage (ADR-0030):** columns are `Vec<T>` behind a `Column` trait object; queries borrow columns with `get_disjoint_mut`. The crate has no `unsafe`, so the Miri step is dropped (no nightly install). Module layout: `storage/{column,table,archetype}.rs`, `query/{mod,data,filter}.rs`, `dynamic.rs`, `resource.rs`, `entity.rs`, `component.rs`, `schedule.rs`, `sim.rs`, `hash.rs`.
  - API: `spawn` returns `Result<EntityId>`; `spawn_with_id` returns `Result<EntityWorldMut>`. Queries take `&mut World` and use closures: `QueryState::for_each(world, |id, item| ..)` and `par_for_each(world, jobs, chunk, ..)`; the entity id is always passed. `QueryState::new` returns `Result` and rejects any repeated component. Query data: `&T`, `&mut T`, tuples up to 4; filters `With`, `Without`, `()`, tuples.
  - `state_hash()` and `clone()` need no registry (columns know their types); `World: Clone` replaces `clone_with`. `component_names(id)` needs no registry.
  - Dynamic access: `get_dyn`, `get_json`, `set_json` (validates via the registry), `insert_dyn`, `remove_dyn`; `ReflectComponent` carries only `insert` (remove/get are by name inside the world). Resources must be `Clone + Send + Sync`; added `resource_scope`.
  - Extended earlier crates: `DynReflect::into_any` (reflect) and `SimRng::seed` (core), each test-first.
  - Extra tests: `spawn_assigns_ids_from_source`, `operations_on_missing_entity_are_errors`, `get_mut_edits_in_place`, `component_names_are_sorted_stable_names`, `cloned_world_is_independent`, `query_on_unknown_component_matches_nothing`, `hash_covers_clock_and_rng`, `systems_run_in_declared_order`.
- **Done criteria:** tests green in debug and release; `just perf-smoke` passes; `just bench` produces a baseline committed to `crates/concordia-ecs/benches/BASELINE.md` (numbers + machine); Miri dropped: no `unsafe` (ADR-0030); `AGENTS.md` lists storage invariants.

### Phase 0.4: concordia-command

- **Objective:** a single path for all edits: attributed, atomic transactions with undo/redo and a persisted, replayable journal.
- **Deliverables:** `Actor`, `EditOp` (serializable data, not closures), `Transaction`, `CommandLog` with linear undo/redo, `Journal` (JSON Lines), `replay_journal`, `CommandStack` façade used by `concordia-api`.
- **Tests first**
  - `tests/actor.rs`
    - `actor_parses_and_displays_canonical_forms` — `human:local`, `agent:gameplay-programmer#2`, `agent:claude-code`, `system:scene-loader`, `tool:concordia-cli` round-trip.
    - `actor_rejects_malformed_with_hint` — `"claude"` → `ActorError::Malformed { input, hint: "expected <kind>:<name>[#n], e.g. agent:claude-code" }`.
    - Added in phase 0.0: `actor_parses_script_kind`, `actor_instance_allowed_on_every_kind`, `actor_rejects_empty_name_and_zero_instance`.
  - `tests/txn.rs`
    - `committed_txn_records_actor_label_and_ops` — begin/apply 3 ops/commit → one log entry with actor, label, 3 ops, `hash_before`, `hash_after`.
    - `failed_op_rolls_back_whole_txn` — op 3 of 3 fails (unknown entity) → world hash equals pre-txn hash; error identifies op index.
    - `abort_restores_world` — `abort()` after 2 ops → hash equals pre-txn hash; no log entry (journal has an `abort` record for audit).
    - `implicit_single_op_txn_when_none_open` — `apply_one(actor, op)` creates and commits a txn labelled by the op.
    - `nested_begin_is_rejected` — `TxnError::AlreadyOpen { txn, actor }`.
  - `tests/undo_redo.rs`
    - `undo_restores_previous_component_value`.
    - `undo_of_spawn_despawns_and_redo_respawns_same_id`.
    - `undo_of_despawn_restores_all_components_and_same_id`.
    - `redo_stack_cleared_by_new_commit`.
    - `undo_redo_are_recorded_with_actor` — the journal holds `undo { target, actor }` and `redo { target, actor }` entries; undo by a different actor is allowed and attributed to that actor.
    - `nothing_to_undo_is_an_error_with_suggestion`.
    - `undo_then_redo_hash_roundtrip_proptest` — proptest: random op sequences, undo all → initial hash; redo all → final hash.
  - `tests/journal.rs`
    - `journal_appends_one_json_line_per_event` — commit, undo, redo, abort, save → 5 lines, each valid JSON with `v: 1`, monotonically increasing `seq`.
    - `journal_tolerates_truncated_last_line` — append garbage half-line; `Journal::load` returns all complete entries + `JournalWarning::TruncatedTail`.
    - `journal_entry_snapshot` — `insta` JSON snapshot of a commit entry (deterministic ids).
  - `tests/replay.rs`
    - `replaying_journal_from_base_snapshot_reproduces_hash` — record a session; replay its entries on a fresh world loaded from the base state → final hash equals recorded `hash_after` of the last entry.
    - `history_reloads_when_scene_hash_matches` — `CommandLog::resume(journal, current_hash)` restores undo stack when last `hash_after == current_hash`; otherwise returns `History::Stale` (undo unavailable, with reason).
- **Implementation notes**
  ```rust
  pub struct Actor { pub kind: ActorKind, pub name: ActorName, pub instance: Option<NonZeroU32> } // Display/FromStr: "<kind>:<name>[#n]"
  pub enum ActorKind { Human, Agent, System, Tool, Script }            // matches the plans/README.md grammar
  #[derive(Reflect, Serialize, Deserialize, Clone, PartialEq)]
  pub enum EditOp {
      Spawn { id: EntityId, components: BTreeMap<String, serde_json::Value> },
      Despawn { id: EntityId },
      SetComponent { id: EntityId, component: String, value: serde_json::Value },   // insert or replace
      RemoveComponent { id: EntityId, component: String },
  }
  pub struct AppliedOp { pub op: EditOp, pub inverse: Vec<EditOp> } // inverse computed at apply time
  pub struct Transaction { pub id: TxnId, pub actor: Actor, pub label: String, pub ops: Vec<AppliedOp>,
                           pub hash_before: WorldHash, pub hash_after: WorldHash }
  pub struct CommandLog { done: Vec<Transaction>, undone: Vec<Transaction>, open: Option<OpenTxn>, journal: Option<Journal> }
  impl CommandLog {
      pub fn begin(&mut self, actor: Actor, label: impl Into<String>, ids: &mut impl IdSource) -> Result<TxnId, TxnError>;
      pub fn apply(&mut self, world: &mut World, reg: &TypeRegistry, op: EditOp) -> Result<(), CommandError>;
      pub fn commit(&mut self, world: &World, reg: &TypeRegistry) -> Result<TxnId, TxnError>;
      pub fn abort(&mut self, world: &mut World, reg: &TypeRegistry) -> Result<(), TxnError>;
      pub fn undo(&mut self, world: &mut World, reg: &TypeRegistry, actor: Actor) -> Result<TxnId, CommandError>;
      pub fn redo(&mut self, world: &mut World, reg: &TypeRegistry, actor: Actor) -> Result<TxnId, CommandError>;
      pub fn history(&self, limit: usize) -> Vec<HistoryEntry>;
  }
  ```
  - Ops apply **immediately** to the world (so reads inside a txn see its writes); inverses are applied in reverse on abort/failure/undo. Values are JSON at this layer (agent-facing), converted through `TypeRegistration::from_json` so invalid values fail before touching the world.
  - Journal: `<project>/.concordia/journal/<scene-id>.cmdlog.jsonl`, one line per event: `{"v":1,"seq":12,"kind":"commit","txn":"…","actor":"agent:claude-code","session":"…","label":"…","ops":[…],"hash_before":"…","hash_after":"…","ts":"…"}`. `fsync` on commit by default (`JournalConfig::sync`). `ts` is wall clock and excluded from snapshots via insta redaction.
  - References: Command pattern (Gamma et al.; Nystrom, *Game Programming Patterns*, "Command"); write-ahead logging concepts; JSON Lines.
- **As built (0.4):**
  - One error type, `CommandError` (no separate `TxnError`); `ActorError::Malformed { input, hint }` for every parse failure.
  - `Actor` is a struct (`kind`, `name`, `instance`); names are `[a-z0-9][a-z0-9._-]*`, at most 64 characters. `Author { actor, session }` carries the session into the journal.
  - Signatures: `begin(world, author, label, ids)` (needs the world for `hash_before`); `commit(world, reg)` and `abort(world, reg, reason)` (need the registry to roll back); `apply_one(world, reg, author, label, op, ids)`; `undo/redo(world, reg, author)`; `record_save(author, path, hash)`; `resume(entries, current_hash) -> (CommandLog, History)`.
  - Commit/undo/redo are journaled before they count; a journal write failure rolls the world back. Empty commits are not recorded. `RemoveComponent` on a missing component is an error. Undo/redo are rejected while a transaction is open.
  - Journal: `ts_ms` (Unix ms integer) instead of an RFC 3339 `ts` string; commit entries store `ops: [{op, inverse}]` so resume needs no replay; `Journal::open` cuts a truncated tail; unknown `v` is an error. Replay also checks each commit's `hash_before`.
  - No separate `CommandStack` facade: `CommandLog` is the facade `concordia-api` uses.
  - Extended earlier crates test-first: `WorldHash` hex `FromStr`/serde and `component_from_json` (ecs).
  - Extra tests: `ops_apply_immediately_inside_a_txn`, `invalid_value_fails_before_touching_world`, `apply_and_commit_without_open_txn_fail`, `remove_missing_component_is_an_error`, `undo_of_set_on_missing_component_removes_it`, `undo_is_rejected_while_a_txn_is_open`, `journal_rejects_unknown_version`, `replay_detects_divergence`, plus actor serde and validation cases.
- **Done criteria:** tests green; journal format documented in `AGENTS.md` and `docs/guides/agents-engine-api.md`; no API exists to mutate the world in `concordia-api` except through `CommandLog`.

### Phase 0.5: concordia-scene (+ ADR-0009)

- **Objective:** the text source of truth: canonical, diff/merge-friendly, LLM-legible scene and prefab files with stable ids and byte-stable round-trips.
- **Deliverables:** `docs/adr/0009-scene-text-format.md`; scene/prefab model; canonical RON serializer (our own serde `Serializer`); reader (ron parser + registry-driven `DeserializeSeed`); `format()` (parse + rewrite); `load_into_world` / `save_from_world`; built-in components; `instantiate_prefab` (flattened copy, fresh ids); fixtures and snapshots.
- **Canonical format (example)**
  ```ron
  // engine scene, format 1. Canonical form: `concordia fmt` rewrites this file byte-for-byte.
  Scene(
      format: 1,
      id: "0193a1c2-7e40-7000-8000-000000000001",
      name: "Arena",
      entities: [
          Entity(
              id: "0193a1c2-7e40-7000-8000-000000000002",
              components: {
                  "Name": "Player",
                  "Transform": (
                      rotation: (0.0, 0.0, 0.0, 1.0),
                      scale: (1.0, 1.0, 1.0),
                      translation: (5.0, 0.0, 0.0),
                  ),
              },
          ),
      ],
  )
  ```
  Rules: 4-space indent; one `Entity(...)` block per entity, entities sorted by id (UUIDv7 ⇒ creation order, so new entities append at the end and merge cleanly); component keys are registry stable names, sorted; struct fields sorted alphabetically (source-field reordering never churns diffs); trailing commas everywhere; floats via `ryu` shortest round-trip, always with a `.` (`1.0`), `-0.0` preserved, non-finite rejected; strings escaped minimally; `Option` as `Some(x)` / `None`; enums as `Variant`, `Variant(x)`, `Variant(field: x)`; glam vectors as tuples; final newline; no other comments emitted (a leading header comment is regenerated; user comments elsewhere are not preserved in M0 — documented).
- **Tests first**
  - `tests/roundtrip.rs`
    - `save_then_load_reproduces_world_hash` — build world, save, load into fresh world → equal `WorldHash`.
    - `load_then_save_is_byte_identical_for_canonical_fixtures` — every `tests/fixtures/*.scene.ron` survives load→save unchanged.
    - `stable_ids_survive_roundtrip` — entity ids and scene id unchanged.
  - `tests/canonical.rs`
    - `canonical_scene_snapshot` — `insta::assert_snapshot!` of a scene with all built-ins and an enum-heavy test component.
    - `format_is_idempotent` — `format(format(x)) == format(x)`.
    - `format_normalizes_noncanonical_input` — shuffled keys, missing trailing commas, 2-space indent, `1` for a float → canonical output (fixture `noncanonical.scene.ron` → snapshot).
    - `insertion_order_does_not_change_output` — same entities created in different orders but same ids → identical text.
    - `float_formatting_is_shortest_roundtrip` — `0.1f32`, `1e-7`, `-0.0`, `f32::MAX` cases.
    - `non_finite_float_rejected_on_save` — NaN in a component → `SceneError::NonFinite { entity, component, pointer }`.
  - `tests/proptest_roundtrip.rs` (proptest)
    - `arbitrary_scene_text_roundtrip` — arbitrary scenes (0–50 entities, random subsets of a test component zoo: nested structs, all enum variant kinds, `Option`, `Vec`, `BTreeMap<String,_>`, unicode strings with quotes/backslashes/newlines, finite floats incl. subnormals) → write → read → structurally equal and hash-equal.
    - `canonical_write_is_fixed_point` — `write(read(write(s))) == write(s)`.
    - `reader_never_panics_on_arbitrary_text` — arbitrary strings and mutated valid scenes → `Result`, never panic.
  - `tests/errors.rs`
    - `unknown_component_reports_location_and_suggestion` — `"Transfrom"` → `SceneError::UnknownComponent { name, line, col, did_you_mean: Some("Transform") }`.
    - `invalid_value_reports_line_col_and_expected_schema` — wrong field type → error with 1-based line/col, JSON pointer, and expected shape string.
    - `duplicate_entity_id_rejected`.
    - `newer_format_version_rejected_with_upgrade_hint` — `format: 2` → `SceneError::UnsupportedFormat { found: 2, supported: 1 }`.
  - `tests/prefab.rs`
    - `prefab_roundtrip_byte_identical`.
    - `instantiate_prefab_assigns_fresh_ids_and_remaps_parent_refs` — `Parent` references inside the prefab point at the new ids; deterministic with `DeterministicIdGen`.
- **Implementation notes**
  ```rust
  pub struct SceneDoc { pub format: u32, pub id: SceneId, pub name: String, pub entities: Vec<EntityDoc> }
  pub struct EntityDoc { pub id: EntityId, pub components: BTreeMap<String, Box<dyn DynReflect>> }
  pub struct PrefabDoc { pub format: u32, pub id: PrefabId, pub name: String, pub root: EntityId, pub entities: Vec<EntityDoc> }
  pub fn read_scene(text: &str, reg: &TypeRegistry) -> Result<SceneDoc, SceneError>;
  pub fn write_scene(doc: &SceneDoc) -> Result<String, SceneError>;   // canonical
  pub fn format(text: &str, reg: &TypeRegistry) -> Result<String, SceneError>;
  pub fn load_into_world(doc: &SceneDoc, world: &mut World, reg: &TypeRegistry) -> Result<(), SceneError>;
  pub fn snapshot_world(meta: &SceneMeta, world: &World, reg: &TypeRegistry) -> SceneDoc;
  pub fn instantiate_prefab(p: &PrefabDoc, ids: &mut impl IdSource) -> Vec<EditOp>; // applied via concordia-command
  // builtin.rs (all #[derive(Reflect)], doc-commented, registered by builtin::register(&mut TypeRegistry))
  /// Human-readable entity name. Not required to be unique.
  #[serde(transparent)] pub struct Name(pub String);
  /// Local transform relative to `Parent` (or world if none).
  pub struct Transform { pub translation: Vec3, pub rotation: Quat, pub scale: Vec3 }  // Default = identity
  /// Hierarchy link to the parent entity's stable id.
  #[serde(transparent)] pub struct Parent(pub EntityId);
  /// Linear/angular velocity integrated by `builtin::integrate_velocity` each fixed tick.
  pub struct Velocity { pub linear: Vec3, pub angular: Vec3 }
  pub fn integrate_velocity(world: &mut World, ctx: &SystemCtx); // the default headless schedule
  ```
  - Reading: parse with the `ron` crate's `Deserializer` into the `SceneDoc` via a `DeserializeSeed` that looks up each component key in the registry and calls the erased `deserialize` for the concrete type (so enums/tuples deserialize exactly as serde defines them). Map `ron` span errors to 1-based line/col.
  - Writing: our `CanonicalRonSerializer` implements `serde::Serializer`, buffers struct fields/map entries and sorts keys, formats floats with `ryu`. Writing does not use `ron::ser` (it cannot sort keys).
  - **ADR-0009 content (to write in this phase):** *Decision: confirm RON with the canonical rules above.* Criteria: LLM familiarity, diff/merge friendliness, one entity per block, comments allowed, typed enums/tuples, serde support. Alternatives: JSON (highest LLM familiarity, but no comments or trailing commas, so noisier diffs and edits; also kept as the API wire format), JSON5 (weaker Rust ecosystem), YAML (indentation-sensitive merges, implicit typing pitfalls, very large spec), TOML (awkward for deep nested arrays of tables), KDL (low LLM familiarity), a custom format (no tooling). Mitigations for RON's lower LLM familiarity: agents edit through MCP rather than raw text; schemas and an annotated example served as MCP resources; `concordia fmt` normalizes hand edits; errors include line/col and suggestions. Revisit trigger: if the live gate (and M1/M2 agent work) shows agents editing raw RON incorrectly at a meaningful rate.
  - References: RON specification (ron-rs repository docs); serde data model; RFC 9562.
- **As built (0.5):**
  - **Own type-directed RON reader/writer instead of the `ron` crate** (ADR-0009): a positioned syntax tree (`src/syntax/`), conversion to serde's JSON form guided by `TypeInfo` (`src/convert.rs`), validation via `TypeRegistration::from_json`, and a JSON + `TypeInfo` → RON writer (`src/write.rs`). No `ron` dependency. Modules: `syntax/{lexer,parser}.rs`, `convert.rs`, `read.rs`, `write.rs`, `world.rs`, `prefab.rs`, `builtin.rs`, `model.rs`, `error.rs`.
  - Header comment: `// Concordia scene, format 1. Canonical form: \`concordia fmt\` rewrites this file.` Floats always contain a `.` (`1.0e-7`). Arrays, tuples and glam vectors are written inline when every item fits on one line.
  - Errors: `SceneError::{Parse, UnknownComponent{did_you_mean}, InvalidValue{component, pointer, expected, found, line, col}, DuplicateEntity, UnsupportedFormat, NonFinite, ...}`. Non-finite detection uses the new `concordia_reflect::find_non_finite` (test-first), because `serde_json` turns NaN into `null`.
  - `snapshot_world(meta, world)` needs no registry; `instantiate_prefab` returns `(root, Vec<EditOp>)` and remaps every string equal to a prefab entity id.
  - Added `concordia_core::suggest::did_you_mean` (strsim Jaro-Winkler ≥ 0.8, top 3), test-first, for reuse by the Engine API.
  - Extra tests: `unknown_field_names_the_valid_fields`, `syntax_errors_report_position`, `wrong_document_kind_is_rejected`, `bad_entity_id_is_reported`, `comments_and_header_are_regenerated`, `reader_never_panics_on_mutated_scenes`, `deeply_nested_input_is_an_error_not_a_stack_overflow`, built-in registration/default/integration tests.
- **Done criteria:** ADR-0009 Accepted; all tests green; fixtures cover every built-in; `AGENTS.md` documents the canonical rules and how to add a fixture.

### Phase 0.6: concordia-api

- **Objective:** the one Engine API: typed methods, JSON-RPC 2.0 over stdio and WebSocket, sessions, actor enforcement, edit/play worlds, and errors an agent can act on without a human.
- **Deliverables:** `Engine` (owns registry, edit world, `CommandLog`, scene meta, optional play world + `SimLoop`, `JobSystem`, `LogRing`), `EngineHandle` (cloneable async handle; engine runs on one dedicated task so mutations are serialized), `Method` trait + `MethodRegistry`, all M0 methods, `ApiError` with suggestions, stdio and WebSocket transports, `docs/guides/agents-engine-api.md`.
- **M0 methods** (params/results are `#[derive(Reflect)]` types; `M` = mutating, requires an actor):

  | Method | Kind | Params → Result (abridged) |
  |---|---|---|
  | `session.open` | — | `{ client, actor? }` → `{ session, concordia_version, protocol: 1 }` (sets the session's default actor) |
  | `scene.new` | M | `{ path, name }` → `{ scene_id, path }` (fails if a dirty scene is open unless `discard: true`) |
  | `scene.open` | — | `{ path }` → `{ scene_id, entity_count, history: "restored" \| "stale" \| "none" }` |
  | `scene.save` | — | `{ path? }` → `{ path, bytes, hash }` (journal `save` record; not a txn) |
  | `scene.info` | — | `{}` → `{ scene_id, name, path, dirty, entity_count, components_in_use }` |
  | `entity.spawn` | M | `{ actor?, components: {Name: value}, id? }` → `{ id }` |
  | `entity.despawn` | M | `{ actor?, id }` → `{}` |
  | `entity.get` | — | `{ id, world? }` → `{ id, components: {…} }` |
  | `component.get` | — | `{ entity, component, world? }` → `{ value }` |
  | `component.set` | M | `{ actor?, entity, component, value }` → `{ previous? }` |
  | `component.patch` | M | `{ actor?, entity, component, patch }` → `{ value }` (RFC 7396 merge patch) |
  | `component.remove` | M | `{ actor?, entity, component }` → `{ previous }` |
  | `query.run` | — | `{ with: [..], without?: [..], where?: [{path, op, value}], select?: [..], world?, limit?, cursor? }` → `{ rows: [{id, components}], next_cursor? }` |
  | `schema.get` | — | `{ name? }` → full schema for a type or method, or the index when omitted |
  | `txn.begin` | M | `{ actor?, label }` → `{ txn }` |
  | `txn.commit` / `txn.abort` | M | `{ txn }` → `{ ops }` |
  | `txn.undo` / `txn.redo` | M | `{ actor? }` → `{ txn, label, by }` |
  | `txn.history` | — | `{ limit? }` → `[{ txn, actor, label, ops, kind }]` |
  | `sim.play` / `sim.pause` / `sim.stop` | — | `{ rate_hz? }` → `{ state, tick }` |
  | `sim.step` | — | `{ ticks }` → `{ tick, world_hash }` |
  | `log.tail` | — | `{ since?, level?, limit? }` → `{ records, next }` |

  `world` is `"edit"` (default) or `"play"`. `sim.play`/`sim.step` lazily create the play world with `World::clone_with`; the edit world (the scene) is never touched by simulation; `sim.stop` discards the play world. `where.op` ∈ `eq, ne, lt, le, gt, ge, contains`; `path` is `Component/json/pointer` e.g. `Transform/translation/0`. Rows are sorted by id; `limit` defaults to 100.
- **Tests first**
  - `tests/method_schemas.rs`
    - `every_method_has_generated_param_and_result_schema` — iterate `MethodRegistry`; each has non-empty `description` (from doc comment) and schemas that validate against the 2020-12 metaschema.
    - `method_index_snapshot` — insta JSON snapshot of the method list (names, mutating flag, description first line). Guards the 1:1 MCP mapping.
    - `mutating_methods_accept_actor_field` — every `M` method's params schema has an `actor` property with a `pattern` for the actor grammar.
  - `tests/dispatch.rs`
    - `jsonrpc_request_response_roundtrip` — `{"jsonrpc":"2.0","id":1,"method":"scene.info","params":{}}`.
    - `jsonrpc_batch_and_notifications` — batch arrays; notifications produce no response.
    - `parse_error_and_invalid_request_codes` — −32700, −32600, −32601 (with did-you-mean method suggestion), −32602.
    - `spawn_set_get_roundtrip_via_dispatch`.
    - `mutation_without_actor_and_without_session_default_fails` — `ACTOR_REQUIRED` with suggestion `"pass \"actor\": \"agent:<role>\" or call session.open with an actor"`.
    - `mutation_records_session_default_actor` — after `session.open { actor: "agent:x" }`, `entity.spawn` is attributed to `agent:x`; explicit `actor` overrides.
    - `query_run_filters_and_sorts` — with/without/where/limit/cursor semantics.
    - `component_patch_merges_fields` — patch `{"translation":[1,2,3]}` leaves rotation/scale.
    - `scene_save_open_preserves_history_when_unchanged` — save, reopen in a new `Engine` → `history: "restored"`, `txn.undo` works.
  - `tests/errors.rs` (agent-actionable errors)
    - `unknown_component_suggests_closest_names` — `"Transfrom"` → code `-32002`, `data.kind = "ComponentNotRegistered"`, `data.suggestions[0]` contains `"Transform"`, `data.see = "concordia://schema/index"`.
    - `invalid_value_includes_pointer_expected_schema_and_example` — `data.pointer = "/translation"`, `data.expected` = the field schema, `data.example` = serialized default value.
    - `unknown_entity_suggests_query` — suggestion mentions `query.run` and, if a `Name` matches the string given, the matching id.
    - `txn_conflict_names_holder` — session B mutating while session A holds an open txn → `TXN_CONFLICT` with `data.holder = { session, actor, label, open_for_ms }` and a retry suggestion.
    - `nothing_to_undo_explains_history_state` — mentions `stale` history if applicable.
    - `every_error_kind_has_message_and_suggestion` — iterate `ApiErrorKind::ALL`; constructing each yields non-empty `message` and ≥ 1 suggestion (prevents unhelpful errors creeping in).
  - `tests/sessions.rs`
    - `sessions_get_unique_ids_and_independent_default_actors`.
    - `open_txn_auto_aborts_on_session_close` — world hash restored; journal `abort` with `reason: "session_closed"`.
    - `open_txn_times_out` — configurable (default 120 s; test uses 50 ms) → auto-abort.
  - `tests/stdio.rs` — `stdio_transport_handles_ndjson_requests` (spawn `concordia headless --stdio` via the CLI bin in Phase 0.7; in this phase test `transport::stdio::serve` over an in-memory duplex).
  - `tests/ws.rs`
    - `ws_transport_serves_jsonrpc` — bind `127.0.0.1:0`, connect with `tokio-tungstenite` client using the printed token.
    - `ws_rejects_missing_token_and_foreign_origin` — no token or `Origin: https://evil.example` → HTTP 401/403 (localhost-only by default; mitigates cross-site WebSocket hijacking).
  - `tests/sim.rs`
    - `sim_step_returns_hash_and_leaves_edit_world_untouched`.
    - `sim_step_hash_same_for_threads_1_and_8` — two `Engine`s with `threads = 1` and `8`, same scene, `sim.step { ticks: 600 }` → equal `world_hash`.
    - `play_world_is_rebuilt_from_edit_world_after_stop`.
  - Added in phase 0.0: `merge_patch_rfc7396_examples` (RFC 7396 appendix vectors), `query_where_rejects_bad_pointer`, `ws_binds_localhost_only`.
- **Implementation notes**
  ```rust
  pub trait Method: 'static {
      const NAME: &'static str;            // "entity.spawn"
      const MUTATES: bool;
      type Params: Reflect; type Result: Reflect;
      fn call(engine: &mut Engine, ctx: &CallCtx, p: Self::Params) -> Result<Self::Result, ApiError>;
  }
  pub struct MethodInfo { pub name: &'static str, pub mutates: bool, pub docs: &'static str,
                          pub params: &'static TypeInfo, pub result: &'static TypeInfo, invoke: ErasedInvoke }
  pub struct CallCtx { pub session: SessionId, pub default_actor: Option<Actor> }
  #[derive(Clone)] pub struct EngineHandle { tx: mpsc::Sender<Envelope> }
  impl EngineHandle { pub async fn call(&self, session: SessionId, method: &str, params: serde_json::Value)
                          -> Result<serde_json::Value, ApiError>; }
  pub struct ApiError { pub code: i32, pub kind: ApiErrorKind, pub message: String, pub data: ErrorData }
  pub struct ErrorData { pub kind: &'static str, pub suggestions: Vec<String>, pub pointer: Option<String>,
                         pub expected: Option<serde_json::Value>, pub example: Option<serde_json::Value>,
                         pub see: Option<String> /* MCP resource URI or doc anchor */, pub details: serde_json::Value }
  ```
  - Error code table (documented in `docs/guides/agents-engine-api.md#errors`): −32001 `EntityNotFound`, −32002 `ComponentNotRegistered`, −32003 `ValueInvalid`, −32004 `ActorRequired`/`ActorMalformed`, −32005 `TxnConflict`, −32006 `NothingToUndo`/`NothingToRedo`, −32007 `SceneIo`, −32008 `SceneParse`, −32009 `NoSceneOpen`, −32010 `SceneDirty`, −32011 `NotPlaying`, −32012 `TxnNotFound`.
  - Suggestions: `strsim::jaro_winkler` over registry names, method names, `Name` values; ≥ 0.8 similarity, top 3.
  - The engine task owns all state; transports are async tasks that forward envelopes. `sim.play` drives `SimLoop::advance` from a `tokio::time::interval`.
  - stdio framing: newline-delimited JSON (same as MCP stdio). WebSocket: one session per connection; token generated at startup, printed to stderr and written to `<project>/.concordia/headless.json` (`{ "url", "token", "pid" }`), mode 0600.
  - References: JSON-RPC 2.0 specification; RFC 7396 (JSON Merge Patch); RFC 6901 (JSON Pointer); RFC 6455 (WebSocket); OWASP notes on cross-site WebSocket hijacking.
- **ADRs:** ADR-0031 (Engine API error model) and ADR-0032 (edit world vs play world) are written in this phase.
- **As built (0.6):**
  - `Engine` is synchronous (`call`, `handle_jsonrpc`, `poll`); `Engine::spawn()` moves it to a dedicated thread behind a cloneable `EngineHandle` (std mpsc in, tokio oneshot out). The thread wakes for real-time play and transaction timeouts.
  - Method marker types carry the doc comment (`#[derive(Reflect)]` unit structs); `MethodInfo::of::<M>()` builds the registry; `CallCtx { session, actor, now }` holds the resolved actor (explicit param, else session default; `ActorRequired` for mutating methods without one). Mutating methods all take `actor: Option<Actor>`.
  - Params decode via `concordia_reflect::validate_json` (pointer errors → `InvalidParams` with `expected` and `example`), then serde. Entity references are strings (`EntityRef`) so a name given as an id yields `EntityNotFound` with the matching id suggested.
  - `ApiError { kind, message, data: Box<ErrorData> }` (boxed for small `Result`s); kind-specific fields go in `data.extra` (`holder`, `line`, `col`). `ComponentMissing` maps to `ValueInvalid`.
  - Merge patch is in-house (`merge_patch`, RFC 7396 appendix vectors). Results are always objects (e.g. `txn.history` → `{entries}`, `schema.get` → `{schema}`), so MCP structured output works.
  - `scene.save` refuses while a transaction is open; `scene.new` refuses an existing file; scenes write via temp file + rename. `scene.new`/`scene.open` discard the play world.
  - Extended earlier crates test-first: reflect `Primitive::Pattern`, `validate_json`, `example_json`, raw-identifier field names; command `ACTOR_PATTERN` and `Reflect for Actor` (the regex test found and fixed `#01` being accepted).
  - `.config/nextest.toml`: `terminate-after = 4` so a hung test fails instead of stalling CI.
  - Extra tests: `explicit_transactions_group_ops`, `scene_new_refuses_dirty_scene_unless_discard`, `scene_paths_must_stay_inside_the_project`, `schema_get_returns_types_methods_and_index`, `no_scene_open_suggests_scene_new`, `errors_serialize_as_jsonrpc_error_objects`, `sim_play_and_pause_report_state`, `connection_file_is_private_and_complete`, `log_tail_returns_ring_records_and_scene_saved`.
- **Done criteria:** all tests green; `docs/guides/agents-engine-api.md` written (workflow, method table, error table, examples); no method mutates outside `CommandLog`.

### Phase 0.7: concordia-mcp + concordia-cli

- **Objective:** expose the API to MCP clients with generated schemas, and ship the `concordia` CLI that creates projects and runs the engine headless.
- **Deliverables:** `concordia-mcp` lib + binary (`concordia-mcp --project <dir> [--actor <actor>] [--scene <path>] [--deterministic-ids <seed>] [--threads N]`); `concordia` binary with `new`, `headless`, `mcp`, `schema`, `fmt`; `concordia new` project template including `.mcp.json`; repo-root `.mcp.json` + `tools/mcp/concordia-mcp.sh`; `examples/m0-sandbox`.
- **Tests first**
  - `crates/concordia-mcp/tests/tools.rs` (in-process via rmcp's duplex/async-rw transport, plus one child-process test)
    - `tool_list_is_one_to_one_with_api_methods` — tool names == `MethodRegistry` names with `.`→`_`, minus `session.open` (transport-level; handled by the adapter), sorted.
    - `tool_input_schemas_equal_generated_param_schemas` — for every tool, `input_schema == concordia_api::params_schema(method)` (byte-equal JSON) — proves nothing is hand-written.
    - `tool_output_schemas_present` — `output_schema` equals the generated result schema.
    - `tool_annotations_reflect_mutation` — `read_only_hint` true iff `!MUTATES`; `destructive_hint` true for `entity_despawn`, `component_remove`, `scene_new`.
    - `tool_call_success_returns_structured_and_text_content`.
    - `tool_call_engine_error_is_tool_error_not_protocol_error` — bad component name → `is_error: true`, content text includes message + suggestions + `see` URI (the model can read and self-correct).
    - `default_actor_from_flag_is_applied` — `--actor agent:claude-code` → journal attribution.
    - Added in phase 0.0: `human_actor_rejected_over_mcp` (both the `--actor` flag and a per-call `actor`; plans/README.md rule), `handshake_works_with_protocol_2025_11_25_and_2026_07_28`, `unadvertised_methods_not_reachable` (rmcp 3.11 rejects methods for unadvertised capabilities).
  - `crates/concordia-mcp/tests/resources.rs`
    - `resources_list_includes_schema_index_docs_and_current_scene` — `concordia://schema/index`, `concordia://docs/agent-guide`, `concordia://docs/errors`, `concordia://scene/current`.
    - `resource_templates_resolve_component_and_method_schemas` — `concordia://schema/component/{name}`, `concordia://schema/method/{name}`; reading `…/component/Transform` returns JSON with the doc-string description.
    - `scene_current_resource_is_canonical_ron` — equals `write_scene(snapshot_world(..))`.
  - `crates/concordia-mcp/tests/stdout_hygiene.rs`
    - `stdout_contains_only_jsonrpc_frames` — spawn the binary with `CONCORDIA_LOG=trace`, run a short session; every stdout line parses as a JSON-RPC message; logs appear on stderr only.
    - `runs_with_empty_environment` — spawn with `env_clear()` plus only `PATH`/`HOME` (no `ANTHROPIC_API_KEY` or any other key) → initialize + one tool call succeed.
  - `crates/concordia-cli/tests/cli_new.rs`
    - `concordia_new_creates_project_skeleton` — `concordia.toml`, `scenes/main.scene.ron` (canonical, parses), `.gitignore` containing `.concordia/`, `.mcp.json`.
    - `concordia_new_mcp_json_snapshot` — insta snapshot of `.mcp.json` (path redacted).
    - `concordia_new_refuses_non_empty_dir_without_force`.
  - `crates/concordia-cli/tests/cli_schema.rs`
    - `concordia_schema_index_matches_registry` — `concordia schema` prints the index; `concordia schema Transform` prints the component schema; `concordia schema --method entity.spawn` prints the params schema; `concordia schema --out <dir>` writes one file per type, sorted, deterministic.
  - `crates/concordia-cli/tests/cli_headless.rs`
    - `headless_stdio_serves_jsonrpc` — pipe two requests, get two responses.
    - `headless_ws_writes_connection_file` — `.concordia/headless.json` with url/token.
  - `crates/concordia-cli/tests/cli_fmt.rs`
    - `fmt_check_fails_on_noncanonical_and_fix_rewrites` — exit code 1 with file list for `--check`; rewrite makes it pass.
- **Implementation notes**
  - `rmcp` server: implement `ServerHandler` by hand (not the `#[tool]` macros, which derive schemas from `schemars`): `get_info` (server name `concordia`, instructions = short workflow pointing at `concordia://docs/agent-guide`), `list_tools` from `MethodRegistry`, `call_tool` → `EngineHandle::call(session, method, args)`, `list_resources`, `list_resource_templates`, `read_resource`. Tool `input_schema` is the generated params schema as `Arc<JsonObject>`; `$defs` are inlined or kept local per tool (MCP clients expect a self-contained object schema). Keep the rmcp-specific code in `src/tools.rs` and `src/resources.rs` so a future rmcp major is a two-file change. rmcp 3.11.0 API (verified 2026-11-17): `ServerHandler::call_tool(CallToolRequestParams, RequestContext<RoleServer>) -> Result<CallToolResponse, ErrorData>` (`CallToolResponse` is an enum; return `CallToolResult::success/error(..).into()`; engine errors are `CallToolResult::error`, never protocol errors, so the agent can read them); `get_info() -> ServerInfo` (= `InitializeResult`) must advertise `tools` and `resources` capabilities; `Tool { input_schema: Arc<JsonObject>, output_schema, annotations: Option<ToolAnnotations> }`; `ServiceExt::serve`, `transport::stdio()`, `TokioChildProcess`, async-rw duplex for in-process tests. MCP protocol: `ProtocolVersion::LATEST` is 2026-07-28 (stateless, no `initialize`); `LATEST_WITH_INITIALIZE` is 2025-11-25. We support both (ADR-0033). **ADR-0033** is written in this phase.
  - Component values inside `component_set` are a free JSON value in the schema (a `oneOf` over every component would bloat the tool list); the `component` property carries an `enum` of registered component names and a description pointing at `concordia://schema/component/{name}`. Validation errors return the exact expected schema.
  - Tool descriptions = method doc comments (first paragraph) + one example call; keep the combined `tools/list` under ~12k tokens (retired by ADR-0073: budgets are per agent role now; asserted by a test `tool_list_token_budget` using a 4-chars-per-token estimate).
  - `concordia mcp` and `concordia-mcp` share `concordia_mcp::run(Args)`.
  - `concordia new` writes `.mcp.json`:
    ```json
    { "mcpServers": { "concordia": { "command": "<abs path to engine>", "args": ["mcp", "--project", "."] } } }
    ```
    The repo-root `.mcp.json` (for *this* Claude Code session) uses `tools/mcp/concordia-mcp.sh`, which resolves the repo root from its own path, builds `concordia-mcp` if missing (`cargo build -q -p concordia-mcp 1>&2`), then `exec`s `target/debug/concordia-mcp --project "${CONCORDIA_PROJECT:-$ROOT/examples/m0-sandbox}"`. Verify that Claude Code launches relative `command` paths from the project root; otherwise use `${VAR}` expansion supported in `.mcp.json`.
  - `concordia.toml` M0 keys: `[project] name`, `[scene] main = "scenes/main.scene.ron"`, `[sim] rate_hz = 60, seed = 0`, `[journal] sync = true`.
- **Gate first:** the opening step of this phase writes `tests/gates/m0_headless_mcp.rs` and `tests/src/{bins,mcp_client}.rs`, committed failing, with `.config/nextest.toml` `default-filter = "not binary(/^m0_/)"` so CI stays green; `just gate M0` runs it explicitly. Phase 0.8 removes the filter.
- **As built (0.7):**
  - Gate first: `tests/gates/m0_headless_mcp.rs` + `tests/src/{bins,mcp_client}.rs` committed red (`0f6910c`); now green. Step 13's log check looks for "scene opened" (logs are per process; "scene saved" happened in the first process). The gate also asserts the server name `concordia`.
  - `concordia-mcp`: `ConcordiaServer::start(Options)` (engine thread + one session with the default actor; optional `--scene`), rmcp `ServerHandler` hand-written in `src/tools.rs` with resources in `src/resources.rs` (rmcp is confined there and in `tests/src/mcp_client.rs`). Tool description = doc comment + "Example arguments" (from `example_json`, nulls dropped). The `component` param stays a free string so tool schemas equal generated schemas byte for byte (ADR-0033). `concordia://docs/agent-guide` embeds `docs/guides/agents-engine-api.md`; `concordia://docs/errors` is generated.
  - API additions (test-first): `Engine::scene_text()` / `EngineHandle::scene_text()` (shared with `scene.save`) and a "scene opened" log record.
  - `concordia` CLI: `new`, `headless [--ws --port]`, `mcp`, `schema [NAME|--method M|--out DIR]`, `fmt [--check] [PATHS]`; exit codes 0/1/2. `concordia.toml` is written but not yet read by any command (M0).
  - `examples/m0-sandbox` is hand-written (a generated `.mcp.json` would contain an absolute home path, which the hygiene test forbids in tracked files); `tools/mcp/concordia-mcp.sh` always runs `cargo build -q` (to stderr) so the session uses the current code.
  - rmcp pulls `chrono`, flagged unmaintained by RUSTSEC-2026-0543 (not a vulnerability; required by rmcp and its schemars dependency). Author decision: stay on rmcp with a scoped `deny.toml` ignore, re-checked at every rmcp upgrade; `rust-mcp-sdk` is the evaluated fallback (ADR-0033).
  - Extra tests: `unknown_tool_is_a_protocol_error`, `unknown_resources_are_not_found`, `concordia_schema_out_writes_one_sorted_file_per_type`, `mcp_subcommand_serves_mcp`, `fmt_reports_parse_errors_with_position`, `engine_exposes_current_scene_text_without_saving`.
- **Done criteria:** all tests green; `claude mcp list` (run manually in this repo) shows `concordia` connected; `AGENTS.md` for both crates.

### Phase 0.8: Milestone gate, live-agent check & docs pass

- **Objective:** prove the M0 promise end to end: an agent with no API key creates, edits, undoes, saves, reopens and queries a scene over MCP with no renderer.
- **Deliverables:** `tests/gates/m0_headless_mcp.rs`, `tests/gates/m0_live_check.rs`, `tests/src/{mcp_client.rs,bins.rs}`, `tools/gates/m0_live.sh`, `tools/gates/m0_live_prompt.md`, justfile recipes (`gate M0`, `gate-live M0`), final review of every crate's `AGENTS.md`, `plans/README.md` status update.
- **Tests first** — the layer-1 gate tests below are written at the *start of phase 0.7* (they cannot compile earlier: they need `concordia-api` and `concordia-scene`), committed failing and excluded from the default nextest run until this phase; this phase makes them green, removes the exclusion and adds `just gate M0` to CI. The live checker is written at the start of this phase.
  - `tests/gates/m0_headless_mcp.rs::m0_gate_agent_workflow_over_mcp` (see Milestone gate for the full script).
  - `tests/gates/m0_headless_mcp.rs::m0_gate_sim_is_deterministic_across_restarts_and_threads` — open the saved scene in two servers (`--threads 1`, `--threads 8`), `sim_step {ticks: 300}` → equal `world_hash`.
  - `tests/gates/m0_live_check.rs::m0_live_outcome_check` — `#[ignore = "run via just gate-live M0"]`; reads `M0_LIVE_PROJECT` and `M0_LIVE_RESULT` (claude JSON output) and asserts outcomes (below).
  - `tests/gates/m0_live_check.rs::m0_scene_is_valid` — `#[ignore]`; lighter check for the in-session run (layer 3): every `*.scene.ron` under `M0_LIVE_PROJECT/scenes` parses and is canonical, and its journal has at least one commit attributed to an `agent:` actor.
- **Implementation notes:** `tests/src/bins.rs::concordia_mcp_bin()` builds once (`OnceLock`) with `cargo build -p concordia-mcp` and returns `target/<profile>/concordia-mcp` (respecting `CARGO_TARGET_DIR`). `tests/src/mcp_client.rs` wraps an `rmcp` client over `TokioChildProcess` with helpers `call(tool, json) -> Result<Value, ToolError>`.
- **As built (0.8):**
  - New recipe `just gate-tests M<n>` runs only a milestone's gate tests; `just gate M<n>` = `just ci` + `gate-tests`. CI runs `just gate-tests M-1` and `just gate-tests M0` after its own lint/test/deny steps (instead of re-running `ci` per gate). The nextest default filter is gone, so the M0 gate also runs in the normal suite.
  - `tools/gates/m0_live.sh` strips every `*_API_KEY` / `*_AUTH_TOKEN` variable by pattern (the hygiene test forbids naming specific key variables in tracked scripts), never uses `--bare`, and bounds each attempt with `timeout 900` instead of `--max-turns` (absent from `claude --help` in 2.1.327). Attempts default to 3 (`M0_LIVE_ATTEMPTS`); artifacts go to `target/gate-live/M0/<stamp>-<n>/`.
  - First live run (2026-11-17): the three `claude -p` sessions all succeeded (15 turns, about 38 s each), but the script passed `-q` to `cargo nextest run` (not a nextest flag), so the checker never ran; checked after the fact against the archived artifacts, all three pass. Script fixed and rerun: pass on attempt 1.
  - `m0_live_outcome_check` additionally asserts that the RESULT line does not name the Lamp and that trees sit at y = 0.
  - Root `AGENTS.md` lists the new recipes; crate `AGENTS.md` entry points were refreshed for items added in later phases.
- **Done criteria:** `just gate M0` green locally and in CI; `just gate-live M0` passes at least 2 of 3 consecutive runs on the author's machine (result recorded in this plan's Status line with date and Claude Code version); manual check in this session done (see gate layer 3).

## Milestone gate (the executable acceptance test: exact command and what it asserts)

### Layer 1 — automated, deterministic, runs in CI

```
just gate M0
# = cargo build -p concordia-mcp -p concordia-cli && cargo nextest run -p concordia-tests --test m0_headless_mcp
```

`m0_gate_agent_workflow_over_mcp` spawns `concordia-mcp --project <tempdir> --actor agent:gate --deterministic-ids 7` as a **subprocess with a cleared environment** (only `PATH`, `HOME`; proving no API key is needed) and drives it with an `rmcp` client over stdio:

1. Protocol handshake (rmcp client default, protocol 2026-07-28; the 2025-11-25 `initialize` path is covered in `concordia-mcp/tests/tools.rs`); `tools/list` contains all 24 M0 tools (every API method except `session.open`); every `inputSchema` byte-equals `concordia_api::params_schema(..)` (generated, not hand-written).
2. `resources/read concordia://schema/component/Transform` → JSON Schema whose `description` is `Transform`'s doc comment.
3. `scene_new { path: "scenes/arena.scene.ron", name: "Arena" }`.
4. `txn_begin { label: "build arena" }`; `entity_spawn` ×4 (`Player` with `Name`+`Transform`, `Crate 1..3` with `Name`+`Transform`+`Velocity`); `txn_commit`.
5. `component_set { entity: Player, component: "Transform", value: {translation: [5,0,0], …} }`.
6. `component_set` with `component: "Transfrom"` → `isError: true`, text contains `Transform` as a suggestion (agent-actionable error).
7. `txn_undo` → `component_get` shows Player translation `[0,0,0]`; `txn_redo` → `[5,0,0]`.
8. `entity_despawn { id: Crate 3 }`; `txn_undo` → `entity_get Crate 3` returns the same id and components.
9. `component_patch { entity: Crate 1, component: "Transform", patch: {scale: [2,2,2]} }`.
10. `scene_save` → file exists. Assertions on the file: `insta::assert_snapshot!("m0_gate_arena_scene", text)` (exact bytes; deterministic ids make redaction unnecessary); `concordia_scene::format(text) == text` (canonical).
11. Kill the server; start a new one; `scene_open` → `history: "restored"`; `query_run { with: ["Transform"], where: [{path: "Transform/translation/0", op: "gt", value: 4}] }` → exactly `[Player]`; `query_run { with: ["Name"] }` → 4 rows sorted by id; `txn_undo` in the new process reverts step 9 (persisted history works).
12. Command journal (`.concordia/journal/<scene-id>.cmdlog.jsonl`): `insta::assert_json_snapshot!` of the entry list with `ts`/`session` redacted; asserts kinds in order `commit("build arena", 4 ops)`, `commit(component.set)`, `undo`, `redo`, `commit(entity.despawn)`, `undo`, `commit(component.patch)`, `save`, `undo`, every actor `agent:gate`, no entry for the failed call in step 6.
13. `txn_history` over MCP returns the same sequence; `log_tail` contains a `scene saved` record.

### Layer 2 — live agent, scripted, outcome-asserting (manual/nightly, needs the subscription login)

```
just gate-live M0
# = tools/gates/m0_live.sh
```

The script:
1. `cargo build -p concordia-cli -p concordia-mcp --release`; creates `$(mktemp -d)/live` via `concordia new` (its `.mcp.json` points at the built binary, default actor `agent:claude-code`).
2. Runs Claude Code headless with **no API key in the environment**, MCP tools only:
   ```sh
   env -u ANTHROPIC_API_KEY -u ANTHROPIC_AUTH_TOKEN \
     claude -p "$(cat tools/gates/m0_live_prompt.md)" \
       --mcp-config .mcp.json --strict-mcp-config \
       --allowedTools "mcp__concordia" \
       --disallowedTools "Bash,Edit,Write,NotebookEdit" \
       --output-format json --max-turns 40 > result.json
   ```
   (verify flag names with `claude --help` at execution time; on 2026-11-17 Claude Code 2.1.327 did not list `--max-turns` in `--help`, so if it is gone, bound the run with `timeout` instead). File-writing tools are disallowed so the scene can only be produced through MCP.
3. Prompt (`tools/gates/m0_live_prompt.md`), fixed: using only the `concordia` tools, create `scenes/courtyard.scene.ron` named "Courtyard"; spawn `Lamp` at translation (2, 0, -3); in one transaction labelled `plant trees` spawn `Tree 1..3` at x = −4, 0, 4, z = 5; spawn `Rock`, then undo that; set the Lamp's scale to (1, 2, 1); save; query entities with a Transform whose z > 1; end with a line `RESULT: <names>`.
4. `M0_LIVE_PROJECT=… M0_LIVE_RESULT=result.json cargo nextest run -p concordia-tests --test m0_live_check --run-ignored only`, which asserts **outcomes, not transcripts**:
   - `result.json` has `is_error: false`; its final text contains `RESULT:` naming Tree 1, Tree 2, Tree 3 (any order).
   - The scene file exists, parses, is canonical, has `name: "Courtyard"`, exactly 4 entities named `Lamp`, `Tree 1..3` (no `Rock`), Lamp translation ≈ (2, 0, −3) and scale ≈ (1, 2, 1) (ε = 1e-4), trees at the requested positions.
   - Journal: a commit labelled `plant trees` with 3 spawn ops; an `undo` whose target txn spawned `Rock`; all commit/undo actors start with `agent:`.
5. Up to 3 attempts (the agent is non-deterministic); each run's `result.json`, scene and journal are archived under `target/gate-live/M0/<timestamp>/`. Pass = any attempt passes; the plan Status records attempts used.

### Layer 3 — this session (the literal design gate, manual checklist)

With the repo-root `.mcp.json` registering `concordia` (via `tools/mcp/concordia-mcp.sh`, project `examples/m0-sandbox`), in this Claude Code session on the subscription with no API key set: create a scene, spawn and edit entities, save, and query, using only `mcp__concordia__*` tools; then run `M0_LIVE_PROJECT="$PWD/examples/m0-sandbox" cargo nextest run -p concordia-tests --test m0_live_check --run-ignored only -E 'test(m0_scene_is_valid)'` (a lighter assertion: the saved scene is canonical and its journal is attributed to an `agent:` actor). Use an absolute project path: nextest runs the test from `tests/`. Record the date in Status.

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| ~~`unsafe` bugs in ECS storage~~ | Retired: storage is safe Rust (ADR-0030); stateful proptest against a model and drop-count tests remain. |
| `rmcp` API churn (major versions move fast) | Pin exact version; isolate in `concordia-mcp/src/{tools,resources}.rs`; the gate tests catch breakage on upgrade. |
| Schema/serde drift (schema says one shape, serde emits another) | Derive forbids shape-changing serde attrs; proptest validates serialized values against generated schemas. |
| stdout pollution breaks MCP stdio | Logging to stderr only from `concordia-core`; `stdout_contains_only_jsonrpc_frames` test. |
| Tool list too large for agent context | Free-form component values with schemas on demand via resources; token-budget test. |
| RON less familiar to LLMs than JSON | Agents use MCP, not raw text; schemas + examples as resources; `concordia fmt`; ADR-0009 revisit trigger. |
| Floating-point nondeterminism across thread counts | Per-entity independent math, fixed chunking, ordered reductions; explicit tests at 1/2/4/8 threads. Cross-platform determinism explicitly deferred. |
| Live gate flakiness / Claude Code CLI flag changes | Assert outcomes only; 3 attempts; archive artifacts; flags verified at execution; not a CI blocker (layer 1 is). |
| Concurrent sessions corrupting undo semantics | One open txn holder at a time with conflict errors and timeouts; global linear history in M0. |
| Journal growth | One file per scene; rotation/compaction is an open question; size is trivial at M0 scale. |
| WebSocket exposure | Localhost bind, token, Origin check; documented. |

## Open questions

1. Should journals be committed alongside scenes (review/attribution in git) or stay in gitignored `.concordia/`? M0: gitignored; revisit with Studio (M3).
2. Preserve unknown components (from plugins not loaded) as opaque RON instead of erroring? M0 errors with a suggestion; likely needed by M2 importers.
3. Preserve user comments in scene files? M0 drops them (documented); a lossless CST-based formatter could come later.
4. Selective per-actor undo for multi-agent Studio sessions (M3).
5. `concordia-mcp --connect ws://…` to attach to a running `concordia headless` (needed when the editor and an agent share one engine, M1).
6. Should `sim.*` be allowed to mutate a play world through commands (for scripted test setups) before `input.inject` exists?
7. Is `Parent` enough hierarchy for M0, or do we need `Children` caches and transform propagation now? (Propagation is likely M1 with rendering.)
8. Whether to adopt parallel system scheduling in M1 or keep intra-system parallelism only.

## ADRs to write

- **ADR-0009: Scene text format** (Phase 0.5) — confirm RON + canonical rules; content outlined above.
- **ADR-0009 decision (phase 0.0, author):** confirm RON; write as Accepted.
- **ADR-0030: Own archetype ECS** (Phase 0.3) — why not `bevy_ecs`/`hecs`/`flecs` bindings: control over determinism, reflection-first dynamic access, stable ids, AI-legible small codebase; costs: unsafe storage to maintain.
- **ADR-0031: Engine API error model** (Phase 0.6) — JSON-RPC codes, `ErrorData` with suggestions/pointer/expected/example/see, rule that every error kind ships with a suggestion.
- **ADR-0032: Edit world vs play world** (Phase 0.6) — simulation never mutates the scene; play world cloned on demand.
- **ADR-0033: MCP adapter** (Phase 0.7) — exact `rmcp` pin and upgrade procedure; supported MCP protocol versions (2025-11-25 with `initialize`, 2026-07-28 stateless); rmcp code isolated in two files; engine errors returned as tool errors.

## Effort estimate

| Phase | Estimate |
|---|---|
| 0.1 concordia-core | 3–4 days |
| 0.2 concordia-reflect + derive | 6–8 days |
| 0.3 concordia-ecs | 10–12 days |
| 0.4 concordia-command | 4–5 days |
| 0.5 concordia-scene + ADR-0009 | 5–7 days |
| 0.6 concordia-api | 7–9 days |
| 0.7 concordia-mcp + concordia-cli | 5–6 days |
| 0.8 gate + live check + docs | 3–4 days |
| **Total** | **~43–55 working days (≈ 9–11 weeks)** |

## Proposed convention additions

- **API methods** beyond the README list: `session.open` (transport-level; not exposed as an MCP tool), `scene.new`, `scene.info`, `entity.get`, `component.patch` (RFC 7396), `component.remove`, `txn.abort`, `txn.history`, `sim.stop`. MCP tools: same names with `_`.
- **Optional `actor` param with session default:** the wire request may omit `actor` if `session.open` set a default; the recorded actor is always present. `concordia-mcp --actor` (default `agent:claude-code`). Actor grammar `<human|agent|system|tool|script>:<name>[#n]`; `human:*` is rejected over MCP.
- **`world` param** (`"edit"` default | `"play"`) on read methods; simulation never mutates the edit world.
- **Error model:** JSON-RPC codes −32001…−32012 as listed; `error.data = { kind, suggestions[], pointer?, expected?, example?, see?, details }`.
- **MCP resource URIs:** `concordia://schema/index`, `concordia://schema/component/{name}`, `concordia://schema/method/{name}`, `concordia://docs/agent-guide`, `concordia://docs/errors`, `concordia://scene/current`.
- **Journal:** `<project>/.concordia/journal/<scene-id>.cmdlog.jsonl` (extension `.cmdlog.jsonl`), JSON Lines with `v: 1`.
- **Headless connection file:** `<project>/.concordia/headless.json`.
- **Component stable names:** registry names default to the Rust ident (`Transform`), overridable with `#[reflect(name = "...")]`; scene files key components by stable name, not Rust path.
- **CLI:** `concordia fmt [--check] [paths]`; `concordia schema [NAME] [--method M] [--out DIR]`; global flag `--deterministic-ids <seed>` (tests only).
- **Justfile recipes:** `just gate-live M<n>`, `just perf-smoke`, `just bench`.
- **Cross-crate test package:** `tests/Cargo.toml` package `concordia-tests`; shared helpers in `tests/src/`; live-gate checkers in `tests/gates/m<n>_live_check.rs` marked `#[ignore]`; live scripts in `tools/gates/`.
- **Env vars:** `CONCORDIA_LOG` (tracing filter), `CONCORDIA_PROJECT` (wrapper default project).
- **Docs:** `docs/guides/agents-engine-api.md` (also served as `concordia://docs/agent-guide`).
