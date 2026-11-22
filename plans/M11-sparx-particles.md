# M11: Advanced particles (Sparx)
Status: Planned (ADR-0015) · Depends on: M1 (RHI compute and `DispatchIndirect`, render graph, `concordia-shaders` `ShaderLibrary`, asset hot reload, `render.stats`, `ViewMode`s, goldens); M2 (signals 2.0b, materials 2.3b, scripts 2.6, replay 2.8); M3 Studio (roster, `studio sync`); M10 10.7 or M4 4.3 (indirect draws) and M4 4.1 (perf harness) reused if they land first · Detail level: **Outline; needs refinement after M2** · Estimated effort: ~19 weeks (range 15–25), author plus agents

> Outline plan with phase breakdowns and test lists. Refine in phase 11.0, once M2's material renderer (2.3b) and the port's first levels exist: measure compute and draw throughput on the reference machine, spike the three risky pieces (Slang `#line` attribution, atomics and indirect draws on Metal), fix the gate budgets, and settle the open questions.

## Goal & context
The engine has no particles. M1 left them out, M2 reports the port's particle effects as unmapped (M2's open question on particles), and the default Studio team ships its VFX artist disabled for want of them.

**Sparx** is a GPU particle runtime in the style of Unreal's Niagara, **without** Niagara's node-graph editor, timeline scrubbing or visual debugger. Those tools carry most of Niagara's engineering cost. What stays are the parts AI agents are good at generating: small text files and small pure functions.
- **Hierarchy:** an *effect* (Niagara's System) holds named *emitters*; each emitter runs an ordered stack of *modules* in two stages, Spawn and Update.
- **Attributes:** typed Struct-of-Arrays buffers (`Position`, `Velocity`, `Age`, `Lifetime`, `Id`, plus custom attributes), one GPU region per attribute.
- **Modules:** small stateless Slang functions (`drag`, `curl_noise_force`, `color_over_life`, …) that declare what they read and write and take a flat, typed parameter map.
- **Data interfaces:** five typed hooks (transforms, scene depth for collision, curves, textures, gameplay parameters) through which effects read the world without touching engine internals.

**Four architectural choices** (ADR-0015):
1. **A declarative effect file**, `.vfx.ron`: canonical RON, schema from `concordia-reflect`. The author's notes said JSON or YAML; in this engine text formats are canonical RON (ADR-0009), and the JSON Schema that agents read is generated from the same reflected types (`schema.get`), so agents still get a JSON Schema to write against.
2. **Template code generation:** a string-template assembler concatenates an emitter's module functions into one Spawn and one Update compute kernel in Slang, with `#line` directives so compiler errors name the module file and line. No graph compiler, no bytecode, no scratch pad.
3. **GPU-driven execution:** persistent attribute buffers, alive and dead lists with atomic counters, and sprites, ribbons and meshes drawn with indirect instanced draws. The frame path never reads back from the GPU.
4. **A hot-reload loop for agents:** the asset watcher rebuilds an effect when its files change, and `vfx.validate` and `vfx.stats` report schema, contract and compile errors with file and line, particle counts and GPU time, so an agent iterates on an effect alone.

**Left out on purpose:** a dual CPU/GPU bytecode VM (everything is GPU compute; the rare gameplay-relevant particle is a plain Terra entity in native code), the scratch pad and graph compiler (agents write Slang and RON directly), and multi-pass simulation stages for grids and fluids (Spawn and Update cover most effects; a seam is left, open question 8).

**What the code gives us today:**
- `concordia-rhi` `Command`: `Dispatch`, `DispatchIndirect`, `Draw` and `DrawIndexed` with instance ranges, `PushConstants`, buffer barriers with `INDIRECT_READ` access, `BufferUsage::{STORAGE, INDIRECT}`, `BlendMode::{Opaque, PremultipliedAlpha, Additive}`. There is **no indirect draw**. M10 (10.7) plans `Command::DrawIndexedIndirect { buffer, offset, count }`, and M4 4.3 plans indirect indexed draws for meshlets. Sparx needs the same command plus a non-indexed `DrawIndirect` for vertex-pulled sprites and ribbons (phase 11.7 coordinates).
- `concordia-shaders`: `ShaderLibrary` (named modules that recompile when their files change, keeping the last good version on errors, `ReloadOutcome`), `CompileRequest` (a root `.slang` path, entry, stage, defines, include dirs), `Diagnostic { file, line, col, message }`, the content-addressed cache, and `ShaderError::Layout` checks of Rust structs against Slang.
- `concordia-render`: the render graph (`graph/`), the forward pass (`forward.slang`) with alpha-blended draws, clustered lights (`cluster.rs`, `lighting.rs`), `cull.rs` `Frustum`, `FrameStats` behind `render.stats`, and `ViewMode` (including `overdraw`).
- `concordia-assets`: the `Importer` trait, `AssetDb::register`, `on_files_changed` hot reload with per-asset `generation`, the DDC, and `.material.ron` (ADR-0068).
- Terra: phases and presentation-only components (ADR-0063), signals (ADR-0069), templates (ADR-0070); the scripting tiers (ADR-0061, ADR-0062) write components and raise signals by name.

**Clean room.** Niagara is part of Unreal Engine, which is source-available, not open source. Sparx uses only Epic's public documentation and public talks about Niagara, for its concepts and vocabulary, plus open research and permissively licensed code (see References). No Unreal source may be read, ported or paraphrased (`docs/clean-room.md`). Every Sparx module's `AGENTS.md` lists its references.

## Scope / Non-goals
**Scope**
- The `.vfx.ron` effect format, validated with errors an agent can act on.
- Attribute layouts and SoA GPU buffers, pooled per emitter definition.
- The module contract and a built-in module library of about 20 modules, plus project modules under `assets/vfx/modules/`.
- Template code generation into one Spawn and one Update kernel per emitter, deterministic and cached.
- GPU spawn and update with alive and dead lists, a CPU reference simulator as the test oracle, and no frame-path readbacks.
- Renderers: camera-facing and velocity-aligned sprites with flipbooks and soft depth fade, ribbons, and instanced meshes, all drawn indirectly.
- Five data interfaces: `transform`, `scene_depth`, `curve`, `texture`, `game_params`.
- Gameplay integration: the `Effect` component, burst signals, the simulation clock, and a written determinism and replay policy.
- The agent loop: hot reload, `vfx.validate`, `vfx.stats`, `render.stats` totals, the CLI, and the VFX artist role.

**Non-goals (deferred; see `later-backlog.md`)**
- A node-graph editor, timeline scrubbing, a visual particle debugger and Niagara-style scratch pads.
- A CPU particle VM or a CPU execution path. Gameplay-relevant particles are Terra entities (open question 13).
- Simulation stages: grids, fluids, neighbor grids (open question 8).
- Skeletal-mesh sampling, heightfield queries, audio spectrum and distance-field collision (M9's `concordia-sdf`); each is a later data interface.
- Lighting particles with Photon's translucency volume (M9) and shadows cast by particles.
- Importing Niagara, Unity VFX Graph or Godot particle systems (M5; open question 12).
- Scalability tiers beyond a project-wide spawn scale.

## Dependencies (what must already exist, from which milestone)
| Needed | From |
|---|---|
| Compute pipelines, `Dispatch`, `DispatchIndirect`, storage and indirect buffers, barriers | M1 1.2 |
| `concordia-shaders`: Slang compile, reflection, cache, `ShaderLibrary` hot reload | M1 1.3 |
| Asset database, importers, DDC, file watching and hot reload | M1 1.4 |
| Render graph, forward pass, clustered lights, `capture.screenshot` | M1 1.5–1.6 |
| `render.stats` (`FrameStats`), `ViewMode`, goldens with FLIP, the self-hosted GPU runner | M1 |
| Presentation-only components and phases (ADR-0063); signals (ADR-0069); templates (ADR-0070) | M2 2.0–2.0c |
| Substrate-style materials in the renderer (ADR-0068) | M2 2.3b |
| Script modules that write components and raise signals by name | M2 2.6 |
| Replay v0 and checkpoint hashes (ADR-0072) | M2 2.8 |
| Studio roster, `studio sync`, per-role tool budgets (ADR-0073) | M3 3.3–3.6 |
| Indirect draws in the RHI | M10 10.7 or M4 4.3, if one has landed (else 11.7 adds them, and they reuse it) |
| Per-pass GPU timing (`PassTimer`), `tests/perf/reference.ron`, budget files | M4 4.1, if it has landed (else M10 10.0's minimum, or 11.0 builds it) |
| TAA reactive mask | M4 4.6, optional (sprites write it once it exists) |

## Crates, modules & directories (created / touched)
**Created**
- `crates/concordia-sparx/` *(proposed; open question 4)*: everything that needs no GPU. Modules: `format` (the effect model and `.vfx.ron`), `layout` (attribute layouts), `module` (the module contract, descriptors and the built-in library in `modules/`), `codegen` (the template assembler), `interface` (the `DataInterface` trait and the five built-ins' Slang APIs and CPU mirrors), `reference` (the CPU reference simulator, a test oracle only), `validate` (diagnostics), `engine` (the `vfx` engine module: `[vfx]`, `vfx.validate`, `vfx.stats`, the `Effect` component).
- `concordia-render::sparx` (module): `pool.rs` (attribute pools per emitter definition), `sim.rs` (spawn, update and compaction passes), `sort.rs`, `draw.rs` (sprite, ribbon and mesh renderers), `bind.rs` (data-interface bindings), `telemetry.rs` (the delayed counter ring). It implements the `VfxTelemetry` trait from `concordia-sparx`, so the engine module never depends on the renderer.
- `examples/m11-sparx/`: original gate effects and scenes (below), procedural textures, CC0.
- `tests/gates/m11_sparx.rs`, `tests/gates/m11_sparx_perf.rs`, `tests/gates/m11_live_vfx.rs`; `tests/golden/m11_sparx/`.

**Touched**
- `concordia-rhi` (+ Metal, and Vulkan when it exists): `Command::DrawIndirect` and `Command::DrawIndexedIndirect` (11.7, shared with M10 10.7 and M4 4.3), and a conformance test for atomics on storage buffers.
- `concordia-shaders`: `CompileRequest` stays path-based; generated kernels are written to the DDC and compiled from there (11.4). `Diagnostic` gains nothing: `#line` sets its `file` and `line`.
- `concordia-render`: graph passes `sparx.simulate` (before the forward pass) and `sparx.draw` (after opaque geometry); `FrameStats` gains `vfx: Option<VfxFrameStats>`; `ViewMode` gains `vfx_overdraw`, appended; `overdraw` counts particles.
- `concordia-assets`: no change to the crate; the `vfx` module registers its importers (`EffectImporter`, `VfxModuleImporter`).
- `concordia-cli`: `concordia vfx validate <path>`; `concordia new` writes a commented `[vfx]` table and `assets/vfx/{modules,shaders,textures}/`.
- `concordia-studio`: the `vfx-artist` role's scope and prompt, the `concordia-author-vfx` skill, `team.toml` enables the role; sync snapshots.
- `editor/ui`: regenerated schemas (`just editor-schemas`).
- `plans/M2-port-begins.md` and `plans/M4-unreal-class-visuals.md`: cross-reference notes (written with this plan).

## Phases
### Phase 11.0: Refinement, spikes and measurement
- **Objective:** reconcile with the renderer and RHI as built, retire the three technical risks, and fix the gate's numbers.
- **Deliverables:**
  - the updated plan, with the author's answers to the open questions;
  - spike 1: a generated Slang file with `#line` directives pointing into two source files; a deliberate error in each is reported by `slangc` (through `Diagnostic`) at the original file and line, on the MSL target;
  - spike 2: `InterlockedAdd` on a `RWStructuredBuffer<uint>` across 1M threads on Metal gives the exact count; a prefix-sum compaction of 1M flags costs what, against atomic appends;
  - spike 3: indirect instanced draws on Metal (`drawPrimitives:indirectBuffer:` and the indexed form) from a compute-written argument buffer, with no CPU wait;
  - measurements on the reference M3 Pro: update throughput for 1M particles at 32, 64 and 128 bytes per particle; the fixed cost per dispatch; additive sprite fill at 1080p for 8, 32 and 128 pixel sprites; a bitonic sort of 64k and 256k keys;
  - the gate effects in `examples/m11-sparx/` as files that parse (they fail validation until their modules exist);
  - perf plumbing: reuse M4 4.1's harness, or M10 10.0's minimum, or build that minimum so the others extend it;
  - ADR-0240 (format, module contract, codegen) drafted.
- **Tests first:**
  - `m11_gate_scenes_load_headless`;
  - the gate test files exist and fail (`tests/gates/m11_*.rs`).
- **Done criteria:** budgets are written to `tests/perf/budgets/m11-*.budget.ron`, never to code; each spike has a result in the plan; the open questions are answered or deferred by name.

### Phase 11.1: Effect format and schema validation
- **Objective:** an agent writes a `.vfx.ron` file and gets either a valid effect or errors that name the field and say how to fix it.
- **Deliverables:**
  - reflected types (`deny_unknown_fields`, one-line docs, ranges): `Effect { version, params, emitters }`, `Emitter { capacity, sim_space, bounds, spawn, attributes, spawn_stack, update_stack, renderers, duration, cull }`, `SpawnRule::{Rate, Burst, PerDistance}`, `ModuleUse { module, args }`, `ParamValue::{Lit, Param, Curve, Texture, Random}`, `RendererDesc::{Sprite, Ribbon, Mesh}`, `AttrType::{F32, Float2, Float3, Float4, U32, I32}`;
  - an example, canonical through `concordia-scene`'s document writer (sorted keys; emitter names are map keys, module stacks are ordered lists):
    ```ron
    Effect(
      version: 1,
      params: { "intensity": F32(1.0), "tint": Float4((1.0, 0.55, 0.2, 1.0)) },
      emitters: {
        "embers": Emitter(
          capacity: 4096,
          sim_space: World,
          bounds: (min: (-2.0, 0.0, -2.0), max: (2.0, 6.0, 2.0)),
          spawn: [Rate(per_second: Param("intensity", scale: 200.0)), Burst(at: 0.0, count: 40)],
          attributes: { "heat": F32 },
          spawn_stack: [
            (module: "builtin:vfx-module/spawn_sphere", args: { "radius": Lit(F32(0.4)) }),
            (module: "builtin:vfx-module/init_lifetime", args: { "min": Lit(F32(1.2)), "max": Lit(F32(2.4)) }),
          ],
          update_stack: [
            (module: "builtin:vfx-module/gravity", args: { "acceleration": Lit(Float3((0.0, 1.5, 0.0))) }),
            (module: "builtin:vfx-module/curl_noise_force", args: { "strength": Lit(F32(2.0)), "frequency": Lit(F32(0.7)) }),
            (module: "builtin:vfx-module/drag", args: { "coefficient": Lit(F32(0.8)) }),
            (module: "asset:0193c1d2-…", args: {}),
            (module: "builtin:vfx-module/color_over_life", args: { "gradient": Curve("asset:0193c1d3-…") }),
          ],
          renderers: [Sprite(material: "asset:0193c1d4-…", blend: Additive, facing: Camera, sort: None)],
        ),
      },
    )
    ```
  - schema validation, all errors collected (not first-only), each a `VfxDiagnostic { kind: Schema, path (JSON Pointer), message, suggestion }`: unknown module or asset, argument names and types against the module's descriptor, ranges (capacity 1..=4,194,304; rates finite and ≥ 0; bounds non-empty), parameter references that exist and match types, duplicate attribute names, reserved built-in names;
  - `EffectImporter` (`.vfx.ron` → validated effect artifact in the DDC), registered by the `vfx` engine module.
- **Tests first:**
  - `effect_roundtrips_canonically` (`insta`, every example file);
  - `effect_text_is_canonical_after_any_edit` (`proptest`: parse, mutate, write, parse: same model, same bytes);
  - `unknown_fields_are_errors_with_their_path`;
  - `every_schema_error_is_reported_not_just_the_first`;
  - `module_argument_type_mismatch_names_the_argument_and_expected_type`;
  - `unknown_param_reference_suggests_the_nearest_name`;
  - `capacity_and_rate_bounds_are_enforced` (each bound; no panic);
  - `schema_snapshot_of_vfx_types` (`insta` over the JSON Schema `schema.get` serves);
  - `effect_import_is_deterministic_and_cached` (same artifact key twice).
- **Implementation notes:** reflection has no recursive types, which suits flat stacks. Names follow ADR-0068's rule (`[A-Za-z0-9_-]+`). Run `just editor-schemas` after the reflected types change.
- **Done criteria:** every gate effect parses and validates up to its (not yet written) modules.

### Phase 11.2: Attribute layouts and SoA buffers
- **Objective:** from an emitter's modules and renderers, decide exactly which attributes exist and where each lives in GPU memory.
- **Deliverables:**
  - built-in attributes: `Position: Float3`, `Velocity: Float3`, `Age: F32`, `Lifetime: F32`, `Id: U32` (always present), and the optional `Color: Float4`, `Size: Float2`, `Rotation: F32`, `Mass: F32`, `RibbonId: U32`, `SpawnIndex: U32`;
  - **pruning:** an attribute is allocated only if some module, renderer or data interface reads or writes it; an attribute read but never written is an error unless it has a declared default;
  - `AttributeLayout`: built-ins first in a fixed order, then custom attributes by name; each a region of `capacity` elements of its GPU type (`Float3` at a 16-byte stride, as Slang lays out `float3` in structured buffers; 11.0 may choose packed streams instead);
  - per-definition pools: all live instances of one emitter definition share one buffer set and one dispatch; each instance owns a fixed slice of `capacity` slots (open question 14);
  - the generated Slang declarations of the layout (consumed by 11.4) and a matching Rust description for the CPU reference.
- **Tests first:**
  - `unused_attributes_are_pruned`;
  - `read_without_writer_or_default_is_an_error`;
  - `layout_is_deterministic_and_independent_of_declaration_order` (`proptest`);
  - `layout_matches_slang_reflection` (through `concordia-shaders`' layout check, `ShaderError::Layout` on a mismatch);
  - `bytes_per_particle_is_reported` (the number `vfx.validate` shows);
  - `pool_slices_never_overlap` (`proptest` over instance churn);
  - `pool_grows_by_whole_slices_and_never_moves_live_instances`.
- **Done criteria:** every gate effect has a layout and a byte count.

### Phase 11.3: The module contract and the built-in library
- **Objective:** a module is a small pure Slang function with a descriptor, checked at load, so a stack can't read what nothing wrote or write what it didn't declare.
- **Deliverables:**
  - a module is two files with one stem: `<name>.vfxmod.ron` (descriptor) and `<name>.slang` (one function);
  - the descriptor (reflected): `stages: [Spawn | Update]`, `reads: { attr: type }`, `writes: { attr: type }`, `params: { name: (type, default, range, doc) }`, `interfaces: [name]`, `doc`;
  - the function's signature is generated from the descriptor: `void <name>(in <Name>Reads r, inout <Name>Writes w, <Name>Params p, SparxCtx ctx)`, where `SparxCtx` holds `dt`, the emitter's age, the RNG stream and the bound interfaces. A module can touch only what its structs hold, so undeclared access is a compile error, not a convention;
  - contract checks over a stack: every read has an earlier writer in the stack (Spawn stack, then Update stack across frames), a built-in default or a declared default; stage allowed; interface available; types agree across modules;
  - the built-in library (`concordia-sparx/modules/`, referenced as `builtin:vfx-module/<name>`): spawn shapes (`spawn_point`, `spawn_sphere`, `spawn_box`, `spawn_cone`, `spawn_mesh_surface` later), `init_lifetime`, `init_velocity_cone`, `inherit_velocity`, `gravity`, `drag`, `curl_noise_force`, `vortex_force`, `point_attractor`, `color_over_life`, `size_over_life`, `rotate`, `kill_by_age`, `kill_in_box`, `collide_plane`, `collide_depth` (11.9), `flipbook_frame`;
  - each built-in has a Rust mirror for the CPU reference (`reference::modules`), written from the same descriptor;
  - `VfxModuleImporter` for project modules under `assets/vfx/modules/`.
- **Tests first:**
  - `descriptor_roundtrips_canonically` (`insta`, every built-in);
  - `read_before_write_is_an_error_naming_both_modules`;
  - `writing_an_undeclared_attribute_fails_to_compile` (a fixture module; the diagnostic names its file and line);
  - `spawn_only_module_in_update_stack_is_an_error`;
  - `type_conflict_between_modules_is_an_error`;
  - `every_builtin_module_compiles_alone` (each in a one-module emitter);
  - `every_builtin_module_has_a_cpu_mirror` (and the reverse);
  - `builtin_module_docs_are_present` (every param documented, for agents).
- **Implementation notes:** module functions must be pure: no globals, no `static` state, no buffer access except through `SparxCtx` interfaces. A lint pass over the Slang source rejects `static` and resource declarations, with file and line, like ADR-0061's `check_module_source` does for native scripts. Curl noise: Bridson, Hourihan, Nordenstam, "Curl-Noise for Procedural Fluid Flow" (SIGGRAPH 2007).
- **Done criteria:** all built-ins pass alone and in the gate effects' stacks.

### Phase 11.4: Template code generation
- **Objective:** turn an emitter into one Spawn and one Update kernel, byte-identically every time, with errors traced to module and line.
- **Deliverables:**
  - `codegen::assemble(emitter, layout, modules) -> GeneratedKernels`: a fixed template (header, layout declarations, interface declarations, module functions in stack order, a kernel body that loads the declared attributes, calls each module, and stores the writes), filled by plain string substitution;
  - each module's body is preceded by `#line 1 "<module path>"` and the generated glue by `#line <n> "<effect path>#<emitter>"`, so `Diagnostic.file` and `line` point at the source a human or agent edits;
  - each Diagnostic is mapped to a `VfxDiagnostic { kind: Compile, module, emitter, stage, file, line, col, message }`;
  - generated source is written to the DDC under the BLAKE3 hash of (template version, layout, module sources, interface versions, defines), and compiled through `ShaderLibrary`, so an unchanged emitter never recompiles;
  - emitters with identical stacks and layouts share kernels.
- **Tests first:**
  - `generated_kernels_are_byte_identical_across_runs_and_thread_counts`;
  - `generated_kernels_snapshot` (`insta`, two gate emitters);
  - `module_order_is_preserved_in_the_kernel`;
  - `compile_error_names_the_module_file_and_line` (a fixture module with an error on line 7 reports line 7 of that file);
  - `error_in_generated_glue_names_the_effect_and_emitter`;
  - `unchanged_emitter_hits_the_cache`;
  - `changing_one_module_recompiles_only_its_users`;
  - `identical_stacks_share_one_kernel`.
- **Implementation notes:** no AST, no IR, no optimizer: Slang does the optimizing. The template has a version number that is part of the cache key. If spike 1 fails for MSL, attribute diagnostics by a line map kept beside the generated file.
- **Done criteria:** every gate emitter generates and compiles; cold compile of a typical emitter ≤ 1.5 s and a cache hit ≤ 5 ms on reference (provisional).

### Phase 11.5: The `Effect` component, triggers and the simulation clock
- **Objective:** gameplay starts, stops, bursts and parameterizes effects through ordinary components and signals, and nothing in the simulation ever depends on particles.
- **Deliverables:**
  - `Effect` (reflected component): `effect: AssetRef`, `playing: bool`, `params: BTreeMap<String, ParamValue>` (overrides), `seed: Option<u64>`, `cull: Simulate | PauseWhenHidden`;
  - `VfxBurst { emitter, count }` signal (ADR-0069), raised by gameplay or scripts on the effect's entity;
  - the presentation-only `EffectRuntime` component (`World::register_presentation`): local time, pending bursts, generation; written by a `Presentation`-phase processor `vfx.triggers` that receives `VfxBurst` and `Effect` changes;
  - **the clock:** in the play world, particles advance by the scaled simulation step (`SimClock`), one update per tick, catching up at most `[vfx] max_catch_up_ticks` per frame; hit-stop (`time_scale` 0) freezes them. The edit world never simulates particles (open question 7);
  - **finite effects end deterministically:** an effect's `duration` is computed from its emitters (spawn window plus maximum lifetime), so gameplay can despawn a one-shot with a `Timer`, never by asking the GPU whether it's done;
  - **seeds:** an instance's seed is the first 8 bytes of BLAKE3 over (session seed, entity id, emitter name), or `seed` when set; particle randomness is a PCG hash of (instance seed, particle `Id`, module index, tick);
  - extraction reads `Effect` and `EffectRuntime` into `EffectInstance` records for the renderer, through `SceneCache`.
- **Tests first:**
  - `world_hash_is_identical_with_and_without_the_vfx_module_running` (same scene, same ticks; the renderer present or absent);
  - `effect_runtime_is_presentation_only` (excluded from `state_digest`; a non-presentation processor reading it is a `ScheduleError`);
  - `burst_signal_is_delivered_once_on_the_next_tick`;
  - `script_module_can_burst_and_set_params` (a 2.6 conformance-style script, by name, no WIT change);
  - `effect_duration_matches_its_emitters`;
  - `hit_stop_freezes_particle_time`;
  - `instance_seed_is_stable_across_runs_and_thread_counts`;
  - `effect_component_roundtrips_in_scene_text` (`insta`);
  - `replay_checkpoints_are_unchanged_by_effects` (record and verify a 600-tick replay with effects; ADR-0072).
- **Implementation notes:** this phase needs no GPU. Check during 11.0 that a `Presentation`-phase processor may receive signals under ADR-0063 rule 4; if not, `vfx.triggers` runs at tick end with `&World` (ADR-0072's hooks) and keeps its state beside the play world (`Engine::play_state`).
- **Done criteria:** a scene's effects start, burst and stop by ticks, with the world hash untouched.

### Phase 11.6: GPU spawn and update
- **Objective:** run the generated kernels on persistent buffers, with alive and dead lists, matching a CPU reference, and with no readbacks in the frame.
- **Deliverables:**
  - per pool: attribute regions, a dead list (initially all slots), alive lists A and B (ping-pong), and a counters buffer (alive, dead, spawned, killed, dropped);
  - passes per tick in `sparx.simulate`: (1) **spawn setup**, one thread per instance: spawn count from rules, clamped to free slots, the excess counted in `dropped`; (2) **spawn**, indirect dispatch over the spawned count: take slots from the dead list, run the Spawn kernel; (3) **update**, indirect dispatch over the alive count: run the Update kernel, mark kills; (4) **compaction**: write survivors to the other alive list and kills to the dead list; (5) **finalize**: write indirect dispatch and draw arguments;
  - **deterministic ordering** (open question 5): compaction by a prefix sum, so slot assignment, list order and the image are identical run to run on one machine; atomic counters still hold the totals. Atomic appends stay as a measured alternative behind `[vfx] ordering`;
  - particle `Id` = the instance's spawn counter (a `u32`, wrapping with a test), the RNG key and the sort tie-break;
  - `reference::simulate(effect, ticks) -> ParticleSet`: the CPU reference simulator, running the modules' Rust mirrors in `f32` with the same RNG; **a test oracle only**, never a runtime path;
  - culling: an instance's bounds are tested with `cull.rs` `Frustum`; hidden instances skip drawing, and also simulation when `PauseWhenHidden`;
  - frame-path rule: no `CopyBufferToBuffer` into host memory is waited on during a frame; counters reach the CPU through a ring of three readback buffers, read two frames late (11.10).
- **Tests first:**
  - `gpu_matches_cpu_reference_for_core_modules` (spawn_sphere, init_lifetime, gravity, drag, color_over_life, kill_by_age; 240 ticks; attributes matched by `Id`, relative error ≤ 1e-4; readback in the test only);
  - `gpu_matches_cpu_reference_for_curl_noise` (tolerance set in 11.0 after measuring transcendental differences);
  - `alive_plus_dead_always_equals_capacity` (every tick, `proptest` over spawn and kill rates);
  - `spawns_beyond_capacity_are_dropped_and_counted_never_silent`;
  - `no_slot_is_alive_twice` (readback of the alive list);
  - `two_runs_give_byte_identical_attribute_buffers` (deterministic ordering on);
  - `dead_particles_never_render` (a kill-everything module: zero drawn);
  - `id_wraparound_keeps_rng_and_sort_valid`;
  - `frame_path_has_no_blocking_readbacks` (the recording test backend counts host waits: 0);
  - `instances_of_one_definition_share_one_dispatch` (dispatch count by definitions, not instances);
  - `paused_hidden_instances_resume_where_they_stopped`.
- **Implementation notes:** references: Latta, "Building a Million Particle System" (GDC 2004; state-preserving GPU particles); Gareth Thomas, "Compute-Based GPU Particle Systems" (GDC 2014); Blelloch, "Prefix Sums and Their Applications" (1990) and Harris, Sengupta, Owens, "Parallel Prefix Sum (Scan) with CUDA" (GPU Gems 3, ch. 39); Jarzynski & Olano, "Hash Functions for GPU Rendering" (JCGT 2020; PCG hash).
- **Done criteria:** all tests green on the reference machine; 1M particles update within the 11.0 budget.

### Phase 11.7: Indirect draws in the RHI (shared)
- **Objective:** one indirect draw command set for every GPU-driven user: Sparx, M10's MDI path and M4's meshlet fallback.
- **Deliverables:**
  - if M10 10.7 or M4 4.3 has landed: reuse its command; add only `DrawIndirect` if it is missing;
  - otherwise add both, with the shape M10 planned: `Command::DrawIndirect { buffer, offset, count }` (vertex pulling; four `u32` per draw) and `Command::DrawIndexedIndirect { indices, buffer, offset, count }` (five `u32` per draw); `count > 1` loops single draws where the backend lacks multi-draw;
  - `CommandList::validate` rules (inside a render pass, buffer has `INDIRECT` usage, offsets aligned), the recording test backend, and Metal encoding;
  - an ADR addendum: ADR-0222 (M10) records the shared command if M10 writes it first; otherwise ADR-0241 records it and M10 and M4 reuse it.
- **Tests first:**
  - `draw_indirect_matches_direct_draw` (RHI conformance: identical images);
  - `draw_indexed_indirect_matches_direct_draw`;
  - `compute_written_arguments_draw_without_cpu_wait`;
  - `zero_instance_indirect_draw_draws_nothing`;
  - `indirect_draw_outside_a_pass_is_rejected_by_validate`;
  - `indirect_buffer_without_indirect_usage_is_rejected`.
- **Done criteria:** conformance green on Metal; M10's and M4's plans point at the one command.

### Phase 11.8: Sprite and mesh renderers
- **Objective:** draw particles as sprites and instanced meshes, indirectly, sorted when needed, with materials.
- **Deliverables:**
  - `Sprite { material, blend, facing: Camera | Velocity | Axis(Float3), sort: None | ViewDepth, size_attr, color_attr, flipbook: Option<(columns, rows)>, soft_fade: Option<f32> }`: four vertices per particle, pulled from the attribute regions through the alive list; soft fade against scene depth;
  - `Mesh { mesh, material, orient: Velocity | Rotation | None, sort }`: `DrawIndexedIndirect` over the alive count;
  - **sorting:** a bitonic sort of (quantized view depth, `Id`) keys in compute for `ViewDepth`, per instance or per pool (11.0 picks), deterministic;
  - **materials** (ADR-0068): a renderer names a `.material.ron`. Particles use it with a closure budget of 1: unlit materials (emission × `Color`) and single-slab lit materials, shaded by the clustered punctual lights and the environment; multi-closure materials are downgraded by ADR-0068's deterministic rule and a warning is reported (open question 9);
  - blending: `Additive`, `PremultipliedAlpha` (sorted), and `AlphaClip`;
  - the forward pass: `sparx.draw` after opaque geometry, depth test on, depth write off; `overdraw` counts particles, and `vfx_overdraw` shows only particles;
  - the TAA reactive mask from M4 4.6, written by sprites, if 4.6 exists.
- **Tests first:**
  - `sprite_faces_the_camera_and_velocity` (analytic: projected corners against a CPU reference);
  - `flipbook_frame_selects_the_right_cell`;
  - `soft_fade_reaches_zero_at_the_surface`;
  - `sorted_alpha_sprites_draw_back_to_front` (a two-layer golden, also with equal depths: tie-break by `Id`);
  - `mesh_particles_draw_indexed_indirect` (draw-call count 1 per pool);
  - `multi_closure_material_is_downgraded_with_a_warning`;
  - `lit_particles_receive_clustered_lights` (golden);
  - `renderer_goldens` (`tests/golden/m11_sparx/sprite_*`, `mesh_*`; mean FLIP ≤ 0.01);
  - `vfx_overdraw_view_mode_golden`; `view_mode_order_is_stable`;
  - `no_effects_render_byte_identical_to_before` (the existing goldens are unchanged).
- **Implementation notes:** references: Lorach, "Soft Particles" (NVIDIA whitepaper, 2007); Batcher, "Sorting Networks and Their Applications" (AFIPS 1968); Persson's particle trimming (SIGGRAPH 2012 course) as a measured option for overdraw.
- **Done criteria:** the campfire and shard-burst effects look right by eye; goldens blessed and passing.

### Phase 11.9: Ribbons
- **Objective:** trails and beams: particles joined in spawn order into camera-facing strips.
- **Deliverables:**
  - `Ribbon { material, width_attr, color_attr, uv: Stretch | Tile(f32), facing: Camera | Axis(Float3), max_segments }`; it needs `RibbonId` and `SpawnIndex`;
  - a compute pass sorts each ribbon's particles by `SpawnIndex` (the 11.8 sort, keyed by ribbon then spawn index) and writes strip vertices; `DrawIndirect` draws them;
  - gaps: a ribbon breaks where neighbors are farther apart than a set distance, so killed middles don't bridge;
  - `PerDistance` spawning (11.1) gives evenly spaced trail points from a moving emitter.
- **Tests first:**
  - `ribbon_follows_spawn_order`;
  - `ribbon_width_and_uv_follow_their_attributes`;
  - `killed_middle_particles_split_the_ribbon`;
  - `per_distance_spawn_spaces_points_evenly` (CPU reference);
  - `ribbon_goldens` (mean FLIP ≤ 0.01).
- **Done criteria:** the comet-trail effect follows its moving entity without kinks or bridges.

### Phase 11.10: Data interfaces
- **Objective:** five small, typed, deterministic hooks into the world, each with a Slang API, a CPU mirror and one binding path.
- **Deliverables:**
  - the `DataInterface` trait in `concordia-sparx`: `name`, `slang_api` (declarations the codegen includes), `params` (reflected), `bind(&FrameInputs) -> InterfaceBinding`, `cpu_mirror`; new interfaces extend it, never the codegen;
  - **`transform`:** the effect entity's world transform and last tick's (for `inherit_velocity`), plus named entity transforms passed as `Param` entity ids;
  - **`scene_depth`:** last frame's depth and view-projection; `collide_depth` reprojects, tests and reflects with restitution and friction; particles off screen don't collide (documented);
  - **`curve`:** 1D curves and gradients from text (`ParamValue::Curve`) or assets, baked to 256-entry LUTs in the DDC; sampled with linear filtering;
  - **`texture`:** sample a 2D texture asset (flipbooks, spawn masks) through the bindless table;
  - **`game_params`:** typed values the simulation publishes: an effect param bound to a component field on its entity (`Bind("Health/current")`, a JSON Pointer through reflection), read during extraction and uploaded once per frame. This is how agents pull gameplay state into VFX without engine internals;
  - later interfaces (not in M11): skeletal mesh sampling (M2 skinning), heightfield queries, audio spectrum (`concordia-audio`), distance fields (M9 `concordia-sdf`).
- **Tests first:**
  - `each_interface_matches_its_cpu_mirror` (a probe emitter per interface; readback in the test);
  - `inherit_velocity_uses_last_tick_transform`;
  - `depth_collision_bounces_off_a_plane` (against the analytic plane, within one depth texel);
  - `curve_lut_matches_the_curve` (max error bound);
  - `game_param_binding_follows_a_component_field` (field changes on tick N appear in particles on tick N's frame);
  - `bad_binding_path_is_a_schema_error_naming_the_field`;
  - `new_interface_needs_no_codegen_change` (a test-only interface plugs in through the trait).
- **Implementation notes:** depth collision follows the screen-space approach in Thomas's GDC 2014 talk and Epic's public Niagara documentation on depth-buffer collision (concepts only).
- **Done criteria:** the spark fountain bounces off the floor; the campfire's flames follow a `game_params` intensity.

### Phase 11.11: Hot reload, validation and telemetry for agents
- **Objective:** an agent edits an effect or module, and within about two seconds knows whether it worked, why not, and what it costs.
- **Deliverables:**
  - hot reload: `AssetDb::on_files_changed` re-imports changed `.vfx.ron`, `.vfxmod.ron`, `.slang` and texture files; affected emitters regenerate and recompile; on success the new kernels swap in at the next tick; on failure the **last good kernels keep running** (as `ShaderLibrary` does) and the diagnostics are kept; a layout change resets that emitter's pool;
  - **`vfx.validate`** (MCP `vfx_validate`), read-only, works without a GPU: params `{ path } | { effect: AssetRef }`; result `{ ok, diagnostics: [VfxDiagnostic], emitters: [{ name, bytes_per_particle, capacity, kernels: { spawn, update } }] }`. An invalid effect is a result, not an error, so no new error code;
  - **`vfx.stats`** (MCP `vfx_stats`), read-only: params `{ entity?, effect? }`; result per instance and emitter: alive, capacity, spawned, killed, dropped, GPU µs for simulate and draw (from `PassTimer` where it exists), kernel generation, last reload outcome and its diagnostics, and `counts_frame` (the frame the delayed counts describe);
  - `FrameStats.vfx: Option<VfxFrameStats>` in `render.stats`: totals only (instances, alive, dropped, GPU ms), null without effects;
  - CLI: `concordia vfx validate <path>` prints diagnostics as `file:line:col: message` and exits non-zero on errors;
  - log lines for reloads and failures under `vfx` (visible in `log.tail`).
- **Tests first:**
  - `editing_a_module_reloads_its_emitters_without_restart`;
  - `broken_module_keeps_the_last_good_kernels_running` (frames continue; `vfx.stats` shows the failure);
  - `validate_reports_compile_errors_with_module_file_and_line`;
  - `validate_reports_schema_and_contract_errors_together`;
  - `validate_works_without_a_gpu` (headless, no device);
  - `stats_counts_match_a_readback_two_frames_later`;
  - `render_stats_reports_vfx_totals` (schema snapshot, `insta`);
  - `reload_to_visible_change_within_budget` (≤ 2 s on reference, provisional, `just perf-smoke`);
  - `cli_validate_exit_code_and_format`;
  - `method_schemas_snapshot` (`insta`; schemas generated from reflection);
  - `vfx_methods_exist_only_with_the_vfx_module` (projects without `[vfx]` keep their tool set).
- **Done criteria:** the loop works end to end from a test client over MCP.

### Phase 11.12: Studio enablement (the VFX artist)
- **Objective:** the default team's VFX artist does real work with Sparx, within its tool budget.
- **Deliverables:**
  - `roster/agents/vfx-artist.md`: scope adds `vfx_validate`, `vfx_stats`, and what its prompt now uses to place and trigger an effect in a test scene (`scene_open`, `scene_save`, `schema_get`, `query_run`, `txn_begin`, `txn_commit`, `txn_abort`, `component_patch`, `sim_stop`); write paths `assets/vfx/**` (effects, `modules/`, `shaders/`, `textures/`) and `tests/golden/m11_sparx/**` only if open question 3 says so; the "disabled until M2" note goes;
  - `team.toml`: the `vfx-artist` is enabled (its reason, "particles and VFX arrive in M2", was never true);
  - a new skill `concordia-author-vfx`: the effect format, the module contract, the edit → `vfx_validate` → `sim_step` → `capture_screenshot` (`view_mode: "vfx_overdraw"`) → `vfx_stats` loop, budgets, and when to write a module versus using a built-in;
  - the tech artist keeps materials and the general shader library; the VFX artist uses materials and asks the tech artist for new ones (`studio_ask`).
- **Tests first:**
  - `vfx_artist_scope_names_only_existing_tools` (3.3's rule);
  - `vfx_artist_prompt_and_skills_use_every_scoped_tool` and the reverse (ADR-0073 rule 3);
  - `every_enabled_default_role_stays_under_its_tool_budget` passes with the VFX artist enabled (22k);
  - `studio_sync_snapshots` updated (`insta`: the generated agent and skill files);
  - `vfx_artist_can_author_and_measure_over_mcp` (deterministic agent-gate step: scripted MCP calls from the role's scope create, validate, place and measure an effect).
- **Done criteria:** `studio sync` on a new project yields an enabled VFX artist with the skill, under budget.

### Phase 11.13: Gate
Write the gate tests at the start of the milestone (11.0); this phase makes them pass on the reference machine and updates the plan's Status.

## Milestone gate
`just gate M11` runs `tests/gates/m11_sparx.rs` and `m11_sparx_perf.rs` on original effects in `examples/m11-sparx/` (our own content: procedural textures and meshes, CC0; no recreation of any existing game's effects):
- **campfire:** flame flipbook sprites (lit by nothing, additive), embers with curl noise, sorted soft smoke, intensity bound through `game_params`;
- **spark fountain:** bursts with depth-buffer collision and bounces on a stepped floor;
- **comet trail:** a ribbon behind an entity moving on a scripted path, with `PerDistance` spawning;
- **shard burst:** mesh particles from a `VfxBurst` raised by a script on a timer;
- **million motes:** 1,000,000 particles in one pool, gravity, drag and curl noise (the throughput scene);
- **brazier hall:** 128 campfire instances in an isometric view (the many-instances scene).

It asserts:
1. **Format:** every sample effect round-trips canonically and validates with no diagnostics.
2. **Codegen:** kernels are byte-identical across two runs and across thread counts; the second run is all cache hits.
3. **Correctness:** GPU attributes match the CPU reference for the core modules (11.6) after 240 ticks.
4. **Images:** each scene at fixed ticks matches its goldens within mean FLIP 0.01; two runs give identical images with deterministic ordering.
5. **Determinism policy:** play-world hashes are identical with Sparx running, absent, or reloading mid-run; a 600-tick replay of the brazier hall verifies.
6. **Agent loop:** breaking a module reports its file and line through `vfx_validate` and `vfx_stats` within 2 s, earlier kernels keep drawing, and fixing it swaps the new kernels in without a restart.
7. **No readbacks:** zero blocking host waits in the frame path.
8. **Scale on reference** (provisional, set in 11.0; 1920×1080): million motes update ≤ 1.5 ms GPU (≥ 0.65M particles per ms); its spawn-and-kill churn of 200k particles per second adds ≤ 0.2 ms; the brazier hall's simulate plus draw ≤ 1.5 ms GPU and ≤ 0.3 ms CPU per frame; 250,000 additive 8-pixel sprites draw in ≤ 1.0 ms. Off-reference runs report `NotReference`.
9. **No silent loss:** dropped spawns are counted in `vfx.stats` and logged.

`just gate-live M11` runs `m11_live_vfx_artist_builds_an_effect`: a real `claude -p` VFX artist, on the subscription, takes a task ("a pickup sparkle readable at 10 m, under 0.3 ms GPU"), and the test asserts outcomes: the effect file validates, it is placed in the scene, and `vfx.stats` shows it within budget.

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| Slang's MSL path ignores `#line`, so errors can't be traced to modules | Spike 1 in 11.0; fallback: a line map stored beside each generated file. |
| Atomic appends make slot order, and so blended images, differ run to run | Prefix-sum compaction by default (11.6), measured against atomics in 11.0. |
| Compile times make the agent loop slow | Kernels cached by content in the DDC; identical stacks share kernels; only changed modules' users recompile; a budget test. |
| Overdraw from large alpha sprites dominates the frame | `vfx_overdraw`, GPU ms per emitter in `vfx.stats`, the skill's budget rules; particle trimming as a measured option. |
| Many small instances cost one dispatch each | Pools per emitter definition: one dispatch per definition, not per instance. |
| Scope creep back toward a node graph, a VM or a debugger | ADR-0015 rules them out; open questions name the only seams left (stages, CPU-drawn particles). |
| Simulation starts depending on particles (despawn when done, hit tests) | `EffectRuntime` is presentation-only, enforced by `SimLoop`; durations are computed, never read back; the hash test in the gate. |
| Module functions with hidden state | Generated signatures expose only declared data; a source lint rejects `static` and resources. |
| Duplicating M10's or M4's indirect draws | One command set (11.7), recorded in one ADR, reused by all three. |
| Clean-room contamination (Niagara is an Unreal feature) | Concepts from Epic's public docs and talks only; open papers for every technique; references in each `AGENTS.md`. |

## Open questions
Each is for the author, with a recommendation.
1. **File names:** `.vfx.ron` for effects and `.vfxmod.ron` plus `.slang` for modules, or `.sparx.ron`? *Recommendation:* `.vfx.ron`, which the VFX artist's roster example already uses; Sparx is the system's name, not the file kind.
2. **API names:** `vfx.*` and `[vfx]`, or `sparx.*` and `[sparx]`? *Recommendation:* `vfx.validate` and `vfx.stats`, enabled by `[vfx]`: the domain says what it does, matching the role and the extension (like `script.reload` and `[scripts]`).
3. **Which role gets the tools?** The brief suggested the tech artist, but the roster has a VFX artist whose mission is exactly this. *Recommendation:* the VFX artist gets `vfx_validate` and `vfx_stats`; the tech artist gets none (it keeps materials and the general shader library). Should the VFX artist also write `tests/golden/m11_sparx/**`? Recommendation: no; goldens stay with the tech artist and QA.
4. **Crate split:** one `concordia-sparx` with GPU code inside, or a GPU-free `concordia-sparx` plus a `concordia-render::sparx` module? *Recommendation:* the split, so `vfx.validate` and the CPU reference run headless with no device, and the renderer stays the only GPU owner.
5. **Ordering:** prefix-sum compaction (deterministic images run to run) or atomic appends (the author's sketch; slightly cheaper, nondeterministic slot order)? *Recommendation:* prefix sums by default if 11.0 measures them within 0.1 ms of atomics at 1M particles; atomics stay as an option. Atomic counters are kept either way.
6. **Clock:** one particle update per simulation tick, or per rendered frame with the frame's `dt`? *Recommendation:* per tick in the play world, so `sim_step` then `capture_screenshot` is reproducible; render interpolation between ticks can come later if 60 Hz ticks look choppy at 120 Hz.
7. **Previews in the edit world:** the edit world never simulates. Should the editor get a presentation-only preview clock? *Recommendation:* not in M11: agents preview with `sim_step`, humans with play. Revisit with the editor after the gate.
8. **Simulation stages** (grids, fluids, neighbor queries): when? *Recommendation:* not in M11. `Stage` stays a non-exhaustive enum and the codegen template has one kernel per stage, so a stage can be added later without changing effect files.
9. **Particle materials:** a closure budget of 1 (unlit and single-slab lit), or full Substrate programs, or custom per-effect shading modules in Slang? *Recommendation:* budget 1 in M11, plus a `shade` stage for sprite modules only if the gate effects need it; Photon's translucency volume lights particles in M9.
10. **When does M11 run?** *Recommendation:* after M2 2.3b and the Studio track, independent of M4; before the port's effects are needed in M5. 11.1–11.5 need no GPU and may start any time after M2 2.8.
11. **M2's open question** ("a minimal CPU emitter in M2, or wait for an M4 VFX graph?"): M4 plans no VFX graph kind. *Recommendation:* answer it as "wait for M11 Sparx", with no CPU emitter in M2; the port ledger keeps particles `untranslated` until M11.
12. **Porting particle effects:** should M5's Godot importer map particle process materials to `.vfx.ron` with built-in modules? *Recommendation:* yes, a mapped subset (emission shapes, velocity, gravity, damping, color and scale curves), unmapped parameters reported in the port ledger. Niagara and Unity VFX Graph stay out of scope (M5's non-goals).
13. **Gameplay-relevant particles:** should Terra entities (projectiles, M8 debris below its size cutoff) be drawable by Sparx renderers through an "external source" pool the CPU fills? *Recommendation:* later (backlog). M8's small debris uses a Sparx burst effect instead, with no gameplay effect.
14. **Pooling:** one pool per emitter definition with fixed slices per instance, or a pool per instance? *Recommendation:* per definition, for one dispatch per definition; a pool grows by slices and never moves live instances.
15. **Hook-driven validation:** should `studio sync` also generate a Claude Code hook that runs `concordia vfx validate` after edits under `assets/vfx/`? *Recommendation:* not at first; the skill calls `vfx_validate`. Add the hook if the live gate shows agents forgetting.

## ADRs to write
- **ADR-0015** (design block, written, Proposed): the Advanced particles milestone (M11, Sparx).
- **ADR-0240:** the effect format, the module contract and template codegen (after open questions 1, 2 and 4).
- **ADR-0241:** GPU execution: pools, alive and dead lists, ordering, sorting, telemetry without readbacks, and the indirect draw commands if M10 hasn't recorded them (ADR-0222).
- **ADR-0242:** gameplay integration and determinism: `Effect`, `VfxBurst`, `EffectRuntime` as presentation, the clock, seeds, and the replay policy.
- **ADR-0243:** the data-interface contract and the five built-ins.

## Effort estimate
| Phase | Weeks |
|---|---|
| 11.0 Refinement, spikes, measurement | 1 |
| 11.1 Effect format and schema | 1 |
| 11.2 Attribute layouts and pools | 1 |
| 11.3 Module contract and library | 2 |
| 11.4 Template codegen | 1.5 |
| 11.5 Component, triggers, clock | 1 |
| 11.6 GPU spawn and update | 2.5 |
| 11.7 Indirect draws (0.5 if reused) | 1 |
| 11.8 Sprite and mesh renderers | 2 |
| 11.9 Ribbons | 1 |
| 11.10 Data interfaces | 1.5 |
| 11.11 Hot reload, validation, telemetry | 1.5 |
| 11.12 Studio enablement | 0.5 |
| 11.13 Gate | 1 |
| **Total** | **~19 weeks** (range 15–25) |

## References (appendix)
Public papers, talks, documentation and permissively licensed code only; no Unreal source.
- Epic Games, Niagara public documentation ("Niagara Overview", "Niagara Key Concepts": systems, emitters, modules, parameters, data interfaces, simulation stages): concepts and vocabulary only.
- Epic Games' public GDC and Unreal Fest talks introducing Niagara: architecture at the level presented in the talks only.
- Lutz Latta, "Building a Million Particle System" (GDC 2004): state-preserving GPU particles.
- Gareth Thomas, "Compute-Based GPU Particle Systems" (AMD, GDC 2014): dead lists, sorting, depth-buffer collision.
- William Reeves, "Particle Systems: A Technique for Modeling a Class of Fuzzy Objects" (SIGGRAPH 1983).
- Robert Bridson, Jim Hourihan, Marcus Nordenstam, "Curl-Noise for Procedural Fluid Flow" (SIGGRAPH 2007).
- Guy Blelloch, "Prefix Sums and Their Applications" (CMU, 1990); Harris, Sengupta, Owens, "Parallel Prefix Sum (Scan) with CUDA" (GPU Gems 3, ch. 39, 2007).
- Kenneth Batcher, "Sorting Networks and Their Applications" (AFIPS 1968): bitonic sort.
- Mark Jarzynski, Marc Olano, "Hash Functions for GPU Rendering" (JCGT 2020): the PCG hash.
- Tristan Lorach, "Soft Particles" (NVIDIA whitepaper, 2007).
- Emil Persson, "Graphics Gems for Games: Findings from Avalanche Studios" (SIGGRAPH 2012 course): particle trimming.
- Godot Engine 4 (MIT): GPU particles and particle process material documentation, for M5's mapping (open question 12).
- Slang documentation: `#line` directives, structured buffers and atomics (Apache-2.0 with LLVM exception).
