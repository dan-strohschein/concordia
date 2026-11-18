# ADR-0011: Evolve, the plugin system
- Status: Accepted
- Date: 2026-11-17

## Context
Unreal's plugins let anyone extend that engine without forking it. Concordia's plans already need many optional extensions (Steamworks, MetalFX/DLSS, network relays, model-backed content providers, extra importers) but had no general mechanism, only one-off seams. The author wants a general plugin system named **Evolve**.

## Decision
- **Evolve** is Concordia's plugin system, built in milestone **M6** (`plans/M6-evolve-plugins.md`), after M2 and in parallel with M3.
- A plugin is a directory with an `evolve.toml` manifest (id, version, engine and Evolve API ranges, SPDX license, modules, dependencies, capabilities). Projects enable plugins in `concordia.toml [plugins]` from local paths or pinned git revisions; `evolve.lock` records versions, content hashes and approved capabilities.
- Two forms share one manifest and one versioned extension-point API: **native** Rust plugins (trusted, explicitly approved, hot reload in development via M2's tier-1 host, statically linked when shipping) and **WASM** component plugins (sandboxed and capability-scoped via M2's WASM host, deterministic).
- Extension points: reflected components and resources, systems, importers, Engine API methods (which become MCP tools automatically with generated schemas), editor panels, render passes (native only), `concordia-gen` providers, Studio roles and skills.
- Plugins' world mutations go through the command log as a new actor kind, `plugin:<id>`.
- Scenes keep components of missing or disabled plugins as opaque values and round-trip them byte for byte.
- No plugin may require an LLM API key; model-backed plugins stay opt-in (ADR-0002). Licenses follow the permissive policy (ADR-0022) unless the user approves an override.
- Clean room: designed from public documentation of plugin concepts; no Unreal source.
- A public registry is backlog work.

## Consequences
Later-backlog "optional plugin crate" items become Evolve plugins. The actor grammar gains `plugin` in M6 (`ACTOR_PATTERN` and parser change, with tests). Extension points become a compatibility promise, versioned by ADR-0140.

## Alternatives considered
- Native only: full power, but no sandbox for third-party code.
- WASM only: safe, but cannot add render passes or deep engine features, and is slower for hot paths.
- Cargo features and forks: no per-project enablement, no third-party ecosystem.
