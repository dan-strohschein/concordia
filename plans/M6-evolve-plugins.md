# M6: Evolve (plugin system)
Status: Planned · Depends on: M0 (reflection, command log, Engine API, MCP), M2 (tier-1 native hot-reload host `concordia-script-native`, WASM component host `concordia-script-wasm`, importer framework); can run in parallel with M3 · Detail level: **Needs refinement after M2** · Estimated effort: ~10 weeks (range 8–13), author plus agents

> Outline plan. Phases, tests-first lists and the gate are set; APIs and file layouts are refined when M2's two script hosts exist, because Evolve reuses them rather than building new loaders (no duplicate functionality).

## Goal & context
**Evolve** is Concordia's plugin system: anyone can extend the engine (new components and systems, importers, Engine API methods and MCP tools, editor panels, render passes, content generators, Studio roles and skills) without forking it. It plays the role Unreal's plugins play, designed from public documentation of the concept only (clean room): a text manifest, versioned extension points, dependency resolution and per-project enablement.

Concordia-specific requirements:
- **AI-first:** every plugin is AI-legible (manifest, `AGENTS.md`, doc-commented reflected types, tests). Plugin-added API methods appear as MCP tools automatically, with generated schemas, so an agent can use a plugin the moment it is enabled.
- **Two forms, one manifest:** **native** Rust plugins (trusted, full power, hot reload in development through M2's tier-1 host, statically linked in shipping builds) and **WASM** component plugins (sandboxed, capability-scoped, portable, deterministic).
- **Text and attribution:** manifests, enablement and the lockfile are text; every world mutation a plugin makes goes through the command log, attributed to a `plugin:<id>` actor.
- **Subscription-first:** no plugin may require an LLM API key. Model-backed plugins (e.g. M3 `concordia-gen` providers) are opt-in by definition.
- **Supply chain:** sources are local paths and git (pinned revisions); a lockfile records content hashes; licenses are checked with the same permissive policy as the engine (ADR-0022), with per-project overrides the user approves. A public registry comes later (backlog).

## Scope / Non-goals
**In scope**
- `evolve.toml` plugin manifest; `[plugins]` in `concordia.toml`; `evolve.lock`.
- Resolution: semver ranges for engine, Evolve API and plugin dependencies; dependency order; cycle and conflict errors with suggestions.
- Extension points (versioned): reflected components and resources, systems in named schedule sets, asset importers, Engine API methods (auto MCP tools), editor panels (web UI bundles), render passes (native only), `concordia-gen` providers, Studio roles and skills.
- Native host (reusing `concordia-script-native`), WASM host (reusing `concordia-script-wasm`) with a capability model.
- Trust: native plugins and capability grants need explicit user approval, recorded in the lockfile.
- Unknown-component preservation: scenes keep components of disabled or missing plugins as opaque values and round-trip them byte for byte (resolves M0 open question 2).
- Tooling: `concordia plugin new|add|remove|list|check|build|package`; Engine API `plugin.*`; a Studio skill "write a plugin".
- Dogfooding: at least one first-party importer and one `concordia-gen` provider move behind Evolve.

**Non-goals (M6)**
- A public registry, marketplace, ratings or payments (backlog).
- Binary plugin distribution (native plugins build from source).
- Untrusted native code (native plugins are trusted code; sandboxing is what the WASM form is for).
- Live unloading of plugins whose components are in use in an open play world (stop, disable, restart; M6 handles edit worlds).

## Dependencies (what must already exist, from which milestone)
| Need | From |
|---|---|
| Reflection and registry, schemas, command log with actors, Engine API, MCP adapter, scene format | M0 |
| Editor shell and panel framework, render graph, asset importer registry | M1 |
| Tier-1 native hot-reload host and C-ABI vtable, WASM component host and WIT world, importer framework | M2 |
| `concordia-gen` provider interface, Studio roles and skills (for those extension points; can land after M3) | M3 |

## Crates, modules & directories (created / touched)
```
crates/concordia-evolve/        manifest, lockfile, resolution, trust, orchestration over the two hosts
crates/concordia-evolve-api/    stable extension-point API for native plugins (the only crate plugins link)
wit/evolve/                     WIT world concordia:evolve/plugin for WASM plugins
crates/concordia-api/           plugin.* methods; plugin-registered methods; unknown-component handling
crates/concordia-scene/         opaque components for missing plugins
crates/concordia-command/       actor kind `plugin` (grammar change, ADR-0011)
crates/concordia-cli/           concordia plugin ...
tools/templates/plugin-native/, tools/templates/plugin-wasm/   templates with AGENTS.md and tests
examples/plugins/               first-party sample plugins (native and WASM) used by tests and the gate
```

## Phases

### Phase 6.1: Manifest, project enablement & resolution
- **Objective:** a pure, fully tested model of plugins and their dependencies, before any loading.
- **Tests first:** `manifest_parses_and_validates_required_fields`; `unknown_manifest_field_is_an_error_with_suggestion`; `engine_and_api_version_ranges_are_checked`; `dependencies_resolve_in_topological_order`; `dependency_cycle_is_reported_with_the_cycle_path`; `conflicting_versions_name_both_requirers`; `git_sources_must_pin_a_revision`; `lockfile_roundtrips_and_records_content_hashes`; `lockfile_mismatch_requires_reapproval`; `non_permissive_plugin_license_is_rejected_unless_overridden`.
- **Implementation notes:** `evolve.toml` (`[plugin] id = "vendor.name"`, `version`, `concordia`, `evolve_api`, `license` SPDX, `description`; `[[module]] kind = "native" | "wasm"`, `path`, `targets = ["runtime", "editor", "tool"]`; `[dependencies]`; `[capabilities]`). `concordia.toml [plugins]` lists enabled plugins and sources. `evolve.lock` pins resolved versions, git revisions, BLAKE3 content hashes and granted capabilities.
- **Done criteria:** tests green; formats documented in `concordia-evolve/AGENTS.md`.

### Phase 6.2: Extension-point API
- **Objective:** one versioned surface that both plugin forms use to register what they add.
- **Tests first:** `registration_is_namespaced_by_plugin_id`; `duplicate_names_across_plugins_are_errors_naming_both`; `api_version_mismatch_fails_before_load`; `registered_components_appear_in_schema_index`; `registered_methods_get_generated_schemas_and_mcp_tools`; `systems_run_in_their_declared_set_and_order`.
- **Implementation notes:** `concordia-evolve-api` exposes a `Plugin` trait and a `Registrar` builder (`component::<T>()`, `system(set, name, f)`, `method::<M>()`, `importer(..)`, `panel(..)`, `render_pass(..)`, `gen_provider(..)`). Plugin API methods are named `<plugin-id-last-segment>.<verb>` and require the plugin prefix to avoid collisions; MCP names follow the usual `.` → `_` rule. The WIT world mirrors the subset that WASM can safely offer (no render passes).
- **Done criteria:** tests green; ADR-0140 (extension-point versioning) written.

### Phase 6.3: Native plugin host
- **Objective:** load trusted Rust plugins in development with hot reload, and link them statically for shipping.
- **Tests first:** `native_plugin_loads_registers_and_unloads`; `native_hot_reload_swaps_systems_at_a_tick_boundary`; `unapproved_native_plugin_is_not_loaded`; `shipping_build_links_enabled_plugins_statically`; `plugin_panic_is_contained_and_reported`.
- **Implementation notes:** reuse `concordia-script-native` (C-ABI vtable, copy-on-load); `concordia plugin build` produces the `cdylib`; approval is stored per plugin and content hash in `evolve.lock`.

### Phase 6.4: WASM plugin host & capabilities
- **Objective:** run untrusted plugins safely and deterministically.
- **Tests first:** `wasm_plugin_registers_components_systems_and_methods`; `ungranted_capability_is_denied_with_the_grant_to_add`; `filesystem_access_is_limited_to_granted_dirs`; `fuel_or_epoch_limit_stops_a_runaway_plugin`; `wasm_plugin_systems_are_deterministic_across_threads`.
- **Implementation notes:** reuse `concordia-script-wasm`; capabilities (`fs.read`/`fs.write` scopes, `net`, `api.read`, `api.mutate`, `assets.import`) map to WASI preopens and host-function gates.

### Phase 6.5: Engine integration
- **Objective:** plugins are first-class in the API, scenes, journal and editor.
- **Tests first:** `plugin_list_enable_disable_over_mcp`; `plugin_mutations_are_journaled_as_plugin_actor`; `scene_with_missing_plugin_components_roundtrips_byte_identical`; `missing_plugin_components_are_reported_not_dropped`; `editor_lists_plugin_panels`; `disable_refuses_while_components_are_in_a_play_world`.
- **Implementation notes:** `plugin.list`, `plugin.enable`, `plugin.disable`, `plugin.reload`, `plugin.check`; actor grammar gains `plugin:<id>` (ADR-0011); opaque components keep their RON text and schema hash.

### Phase 6.6: Tooling & templates
- **Objective:** writing a plugin is a few commands and an agent-friendly template.
- **Tests first:** `plugin_new_native_and_wasm_templates_build_and_pass_their_tests`; `plugin_check_reports_manifest_schema_license_and_capability_problems`; `plugin_package_is_reproducible`.
- **Implementation notes:** templates include `AGENTS.md`, a reflected component, a system, an API method and tests; a Studio skill "write a plugin" (M3 format) drives the flow.

### Phase 6.7: Dogfooding first-party plugins
- **Objective:** prove the extension points on real engine features.
- **Tests first:** existing importer and generator test suites pass unchanged with the features loaded as Evolve plugins.
- **Implementation notes:** move one importer (e.g. glTF or FBX) and one `concordia-gen` provider behind Evolve; later-backlog "optional plugin crate" items (Steamworks, MetalFX/DLSS, relays, local-model providers) become Evolve plugins.

### Phase 6.8: Gate assembly, docs pass

## Milestone gate
`just gate M6` (`tests/gates/m6_*.rs`, no API key, over MCP):
1. `concordia plugin add examples/plugins/hello-native` and `.../hello-wasm`; approve the native one; `evolve.lock` records hashes and grants.
2. Over MCP, the agent sees the plugins' new tools in `tools/list`, spawns entities with plugin components, calls a plugin method; the journal attributes the plugin's own edits to `plugin:<id>`.
3. The WASM plugin's denied capability returns an actionable tool error.
4. Edit the native plugin's system and hot-reload it mid-simulation; the change is visible at the next tick.
5. Disable both plugins; the scene still loads, the plugin components survive a save byte for byte, and `plugin.check` lists them as missing.
6. Live layer (`just gate-live M6`): Claude Code on a subscription writes a small WASM plugin from the template, enables it and uses its tool.

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| Extension-point churn breaks plugins | Versioned `evolve_api`; deprecation window; ADR-0140; compatibility tests against sample plugins |
| Native plugins are a supply-chain risk | Explicit approval per content hash; source builds only; license policy; WASM as the default recommendation |
| Rust ABI instability for native plugins | Reuse M2's C-ABI vtable; static linking for shipping |
| Name collisions | Plugin-id namespaces for methods, components and schedule sets |
| WASM determinism | Same rules as M2 gameplay WASM (no host time or randomness; seeded inputs) |

## Open questions
1. Should plugin components be namespaced in scene files (`"acme.Health"`) by default, or only on collision?
2. Do editor panels run in the editor's web UI process with a capability bridge, or in an isolated frame?
3. Registry design (index format, signing, yanking) when the backlog item is picked up.

## ADRs to write
- ADR-0011 (design): Evolve plugin system (written with this plan).
- ADR-0140: Extension-point API versioning and stability.
- ADR-0141: Plugin trust, capabilities and the lockfile.
- ADR-0142: Opaque components for missing plugins.

## Effort estimate
| Phase | Estimate |
|---|---|
| 6.1 Manifest & resolution | 1.5 weeks |
| 6.2 Extension-point API | 1.5 weeks |
| 6.3 Native host | 1.5 weeks |
| 6.4 WASM host & capabilities | 1.5 weeks |
| 6.5 Engine integration | 1.5 weeks |
| 6.6 Tooling & templates | 1 week |
| 6.7 Dogfooding | 1 week |
| 6.8 Gate & docs | 0.5 week |
| **Total** | **~10 weeks (8–13)** |

## Proposed convention additions
- Files: `evolve.toml` (plugin manifest), `evolve.lock` (project lockfile), `[plugins]` in `concordia.toml`; plugin sources under `plugins/` in a project by convention.
- Actor kind `plugin:<id>`.
- API methods `plugin.list/enable/disable/reload/check`; plugin-added methods `<plugin>.<verb>`.
- Crates `concordia-evolve`, `concordia-evolve-api`; WIT package `concordia:evolve`.
- ADR block 0140–0159 for M6.
