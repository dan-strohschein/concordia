# concordia-audio: agent guide

## Purpose
Engine audio (M2 phase 2.7a, ADR-0066): WAV (`hound`) and Ogg Vorbis (`lewton`) decoding, the `sound` and `cue` importers, an own 48 kHz stereo `f32` mixer (buses, voices, stealing, streaming, equal-power spatialization) fed through lock-free `rtrb` rings, and an offline backend whose renders are bit-identical. Audio is presentation-only and never touches a Terra world. `#![forbid(unsafe_code)]`. 2.7a.1 is the library; 2.7a.2 adds the `audio` engine module (`[audio]`), the `cpal` device, the tools and the debug layer.

## Public entry points
- **Decoding:** `decode_wav`, `decode_ogg`, `OggStream` (packet by packet; exact length from the last granule position); `SoundData` (rate, `Channels`, interleaved samples).
- **Assets:** `SoundImporter` (`sound`: `wav`, `ogg`; `SoundSettings { stream, gain_db, loop_start, loop_end }`; Ogg over `STREAM_AFTER_SECONDS` streams) → `Sound` (`SoundBody::Decoded` or `Streamed`) as `AssetKind::Sound`; `LoadedSound` / `LoadedBody` (shared, ready to play).
- **Cues:** `Cue` (`.cue.ron`: weighted `CueSound`s, `volume_db` and `pitch_semitones` `Span`s, `bus`, `priority`, `looping`, `max_instances`, `spatial`, `seed`), `read_cue` / `write_cue`, `Cue::validate` (`CueError::Field` names the field), `Cue::draw(DrawKey)` → `Draw`; `CueImporter` (`cue`) → `AssetKind::Cue`.
- **Buses:** `BusLayout` / `BusDef` (`.buses.ron`; `read_buses` / `write_buses`), `BusGraph::new` (validated; `index`, `parent`, `names`), `BusIndex`, `MASTER`.
- **Mixing:** `Mixer::new(MixerConfig)` → (`Mixer`, `MixerHandle`). `Mixer::render(&mut [f32])` is the audio-thread half. `MixerHandle` has `play(PlayRequest) → VoiceId`, `stop`, `stop_all`, `set_gains`, `set_bus`, `pump_streams`, `collect` and `stats` (`MixerStats`: steals, drops, underruns, active, per-bus peaks).
- **Math:** `db_to_gain`, `gain_to_db`, `semitones_to_ratio`, `equal_power`, `distance_gain`, `Spatial`, `Listener`, `spatial_gains`.
- **Offline:** `OfflineBackend` (`handle`, `render_frames`, `render_tick`, `stats` → `RenderStats { frames, peak, rms, hash }`, `start_capture` / `keep_last` / `take_capture` / `captured`) and `tick_frames`; `write_wav16`.
- **Engine (2.7a.2):** `AudioModule::new(AudioSettings { voices, buses, backend })` (`Backend::Offline`, `Device(Arc<dyn AudioDevice>)`, `None`), its `AudioShared` (also `Engine::state::<AudioShared>()`; `lock()` → `AudioCore`: `backend_name`, `handle`, `buses`, `library`, `playing` → `VoiceInfo`, `listener`, `render_stats`, `capture`, `stop_all`), `AudioLibrary`, `Collector` (`audio.collect`), the components and signals (`AudioEmitter`, `AudioListener`, `AnimEventCues`, `PlayCue`, `StopCue`; `register_types`), the methods (`AudioPlay`, `AudioStop`, `AudioBuses`, `AudioRender`, `AudioInspect` and their params and results), `AUDIO_LAYER` and `debug_geometry`, `CpalDevice`, and `importers()` (what projects register).

## Invariants
- **`Mixer::render` never allocates, frees, locks or blocks** (`audio_callback_never_allocates`). Voices are built and boxed by `MixerHandle::play`. Finished, stolen and dropped voices go back through the retired ring and are dropped in `MixerHandle::collect`, on the engine thread.
- **Unpositioned voices aren't panned:** mono goes to both channels unchanged and stereo plays as is. A `spatial` stereo voice is averaged to mono before its gains. Gains are never negative.
- **Stealing:** a free slot, else the lowest priority, oldest first, if the new voice outranks or ties it; otherwise the request is dropped (counted). `max_instances` ramps out the group's oldest counting voice.
- **Stops are a 240-frame linear ramp:** frame k is (240 − k)/240.
- **Buses sum children-first** (deepest first, then by index), Master last. Master always exists; a bus with no send goes to Master; a cycle, an unknown send, a duplicate or Master with a send is an error naming the bus.
- **Streams play exactly their decoded twin's frames:** the feeder loops by re-opening the reader and skipping to `loop_start`; the final page's granule position trims the last block. Offline, streams are refilled before every block, so they never underrun.
- **Offline ticks are sample-exact:** tick *t* renders [⌊t·rate/tick_rate⌋, ⌊(t+1)·rate/tick_rate⌋); commands queued before it apply at its first frame.
- **Cue draws** use only `seed ⊕ entity ⊕ (tick × 2⁸) ⊕ n` and SplitMix64 (the sound, then the volume, then the pitch): no clock, thread or world RNG.
- **Audio lives outside the world (ADR-0066):** gameplay's requests (`AudioEmitter`, `AudioListener`, `AnimEventCues`, pending `PlayCue` / `StopCue`) are hashed simulation state; voices, the mixer, runtime buses, draws, streams, previews and the capture are in `AudioShared`, never in the world. `audio.collect` runs in `Phase::Presentation`, reads and receives only, and writes nothing (`audio_is_presentation_only_and_never_changes_the_world_hash`: with and without it, every tick's hash matches at 1 and 8 threads).
- **The processor, each tick:** gather (`Transform`, `Parent`, emitters and listeners sorted by id, `AnimEventCues`, signals), lock the core, prune retired voices, pick the listener (lowest id with a `Transform`), start and stop emitters (a one-shot restarts only when `playing` turns true; a despawned or off emitter ramps out), handle `StopCue`, then `PlayCue` and mapped `AnimEvent`s (draw key: the entity folded to 64 bits, the tick, a per-entity count), re-pan positioned voices, and offline render the tick (relative to the first tick of play).
- **Play start** loads every imported cue and its sounds, makes a fresh offline render (keeping the newest 30 s) and re-applies runtime bus settings; dropping the play state stops every voice. A device that fails to open is one warning and offline mixing.
- **The device:** cpal's stream lives on `concordia-audio-device` (dropped with the guard); its callback renders in 4,096-frame blocks into preallocated scratch and converts (stereo to the first two channels, the average to mono). `concordia-audio-stream` refills streams and frees retired voices every 50 ms while the core lives.

## How to test
On Linux, building needs ALSA's headers (`libasound2-dev`, found through `pkg-config`) for cpal's backend; CI installs them in every Linux job that builds the workspace. macOS needs nothing extra.

`cargo nextest run -p concordia-audio`. Every test header derives its expected values by hand, using DC and ramp sources:
- `tests/math.rs`: dB, pan, distance, listener, tick frames;
- `tests/decode.rs`: WAV written in the test with `hound`; `tests/fixtures/audio/tone-440.ogg` from `tools/fixtures/make_audio.sh`;
- `tests/import.rs`: `AssetDb` and the DDC;
- `tests/cue.rs`: SplitMix64's published outputs;
- `tests/mixer.rs`: sums, buses, interpolation, loops, ramps, stealing, groups, spatial, validation;
- `tests/stream.rs`: streamed and decoded renders are equal;
- `tests/offline.rs`: ticks, stats, bit-identical renders;
- `tests/no_alloc.rs`: `allocation-counter`, dev-only (A2).
- `tests/engine.rs` and `tests/api.rs` (2.7a.2) run an engine with `[assets]`, `[audio]` and a gameplay test module (`tests/rig`: `test.Bell` raises `PlayCue`, `test.Hush` `StopCue`, `test.Doom` despawns; every asset's sidecar is pinned so two rigs hash alike): emitters, signals, the listener, the hash boundary, replays, a failing device, the methods and the debug layer.
- `tests/device.rs`: `cpal_device_plays_a_cue`, `#[ignore]` (manual, on a machine with speakers: `--run-ignored only`).
- Clip events and a WASM guest raising `PlayCue` are tested in `concordia-app`'s `tests/audio.rs`.

## Common tasks
- **A new import setting:** add it to `SoundSettings` (`#[serde(default)]`), check it in `SoundSettings::check`, carry it on `Sound`, and bump `SoundImporter::version` if output changes.
- **A new mixer command:** a `Command` variant, built and validated in `MixerHandle`, applied in `Mixer::apply_commands` without allocating, and covered by `no_alloc`.

## Don'ts
- Don't allocate, drop an `Arc`/`Box`/`Vec`, lock, log or block inside `Mixer::render` or anything it calls. Retire instead.
- Don't use a clock, thread id or global RNG for variation.
- Don't add an MPL (or other non-permissive) decoder; `symphonia` stays out.

## References
- **Formats:** the RIFF/WAVE format (Microsoft/IBM, 1991) and the Vorbis I specification (Xiph.Org), including its granule-position end trim.
- **Crate documentation:** `hound`, `lewton`, `rtrb` and `allocation-counter`.
- **cpal** (Apache-2.0): its documentation, for the device, stream and sample-format API.
- **DSP:** decibels as 20·log₁₀ of an amplitude ratio; the equal-power cos/sin pan law (Roads, *The Computer Music Tutorial*, 1996); inverse-distance-clamped attenuation (the OpenAL 1.1 specification); linear interpolation.
- **SplitMix64:** Steele, Lea and Flood, 2014; its published outputs for seed 0.
- **Godot (MIT):** the `AudioStreamPlayer` and audio-bus docs, for semantics only.
- No Unreal, Wwise, FMOD or other non-open source was consulted.

## Status
M2 phase 2.7a.1 (library) and 2.7a.2 (the engine module, components and signals, `cpal`, the `audio.*` tools and the debug layer; the sound designer and composer in `concordia-studio`). DSP effects, HRTF, Doppler, occlusion, crossfades and UI sounds are later work.

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
