# concordia-rhi-select: agent guide

## Purpose
The one place that turns a backend choice into a GPU device (ADR-0088, M4 4.1v(b)). Everything that opens a device (the app's render and pathtrace modules, `concordia gpu-probe`, GPU tests) goes through it instead of naming a backend crate.

## Public entry points
- `BackendChoice::{Auto, Metal, Vulkan}` (serde and `FromStr` by lowercase name; `resolve`, `resolve_for(macos)`)
- `choose(env, project)` and `choose_from_env(project)` → `Backend`; `BACKEND_ENV` (`CONCORDIA_BACKEND`); `SelectError`
- `new_headless(Backend)`, `new_headless_masked(Backend, &CapsMask)`, `probe(Backend)`, `probe_record(Backend)` (JSON)

## Invariants
- Precedence: `CONCORDIA_BACKEND` (empty is unset), then the project's `[render] backend`, then `auto` (Metal on macOS, Vulkan elsewhere). An unknown env value is an error naming the variable, never a silent fallback.
- No `unsafe`, no backend logic: it only dispatches to `concordia-rhi-metal` and `concordia-rhi-vulkan`, both of which build (and report `NoDevice`) everywhere.

## How to test
- `cargo nextest run -p concordia-rhi-select`: parsing and precedence run everywhere; the device tests accept `NoDevice` unless that backend is required (`concordia_testkit::device_required`).
