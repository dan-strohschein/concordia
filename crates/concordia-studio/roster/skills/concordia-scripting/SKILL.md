---
name: concordia-scripting
description: "Writes deterministic gameplay modules with the Concordia guest SDK (components, signals, traits and processors), builds them as WASM or native libraries, and loads them into the running engine with script_reload."
when_to_use: "Use when gameplay behavior belongs in a script: new processors, new script components or signals, or a change to an existing module under scripts/."
user-invocable: false
---
# Gameplay scripts

A gameplay module is a Rust crate under `scripts/<module>/` written with `concordia-script-guest`. It declares its own components, signals and traits, and processors that run once per tick in a phase, over every entity their query matches. The engine runs it as a WASM component (sandboxed) or as a native library (trusted, hot-reloadable); `concordia.toml`'s `[scripts]` lists the built files.

## Steps
1. See what is loaded: `mcp__concordia__schema_get {}` lists components (script types are named `<module>.<Type>`).
2. Write the module (see `examples/m2-scripting/scripts/wanderers`):
   - components and signals are `#[derive(Reflect)]` types with `#[reflect(name = "<module>.<Type>", default)]`;
   - `describe()` lists them, any traits, and processors: `processor("mod.walk", Phase::PostPhysics).reads::<Velocity>().writes::<Position>().build()`;
   - `run(name, ctx)` reads whole columns (`ctx.read()`), writes them back (`ctx.write(&values)`), and records commands (`ctx.spawn_template`, `ctx.insert`, `ctx.signal`, `ctx.despawn`);
   - end with `export_module!(MyModule);`.
3. Keep it deterministic: no statics or thread-locals (state lives in components; native modules are checked), time from `ctx.dt()`, randomness from `ctx.rng()`, trigonometry from `concordia_script_guest::math`.
4. Build: `cargo build -p <module> --target wasm32-wasip2 --release` (WASM) or `cargo build -p <module> --features dylib` (native), and copy the file to the path `[scripts]` names.
5. Load it: `mcp__concordia__script_reload {}` (or `{"module": "<name>"}`). It never builds: it loads what is on disk. Changed component layouts are migrated: in the edit world as one undoable transaction by `script:<module>` (`txn_history` shows it), and in the play world.
6. Check it: `mcp__concordia__sim_step {"ticks": 60}`, `mcp__concordia__query_run {"with": ["<module>.<Type>"], "world": "play"}`, and `mcp__concordia__log_tail {"level": "info"}` for script log lines and faults (a faulting processor is disabled until the next play session or reload).

## Sound from a script
Play a cue by raising the engine's `PlayCue` signal on an entity (`ctx.signal(id, &PlayCue { .. })`): it sounds at that entity's position on tick t + 1, and never changes the world hash beyond the signal itself. `StopCue` stops what that entity started. Mirror both field for field (names resolve against the engine's registry; an asset reference travels as its string), and ask the sound designer for the cue id. Clip events need no code: the sound designer maps them with `AnimEventCues`.
```rust
/// The engine's `PlayCue`, mirrored.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "PlayCue", default)]
pub struct PlayCue {
    pub cue: String,
    pub volume_db: f32,
}

/// The engine's `StopCue`, mirrored (none: everything this entity started).
#[derive(Debug, Clone, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "StopCue", default)]
pub struct StopCue {
    pub cue: Option<String>,
}
```

## Driving the UI
The UI/UX designer builds screens; your code fills them and reacts to them. Write a HUD value the same way as any component, e.g. set a health bar's `UiProgress.value` or a label's `UiText` runs (`ctx` commands on the widget's entity, mirrored like `PlayCue` below). Controls raise signals on their own entity the tick after they are used: receive `UiPressed` (a button), `UiToggled { on }`, `UiValueChanged { value }` (a slider) and `UiSelected { index }` (a dropdown or tabs); or just read `UiSlider.value` / `UiToggle.on` each tick. Ask the designer which entities they are.

## Output contract
In the task comment: the module and processors you changed, the build command, the `script_reload` result (version, migrated types), and the play-world check.
