# concordia-audio: agent guide

## Purpose
Engine audio (M2 phase 2.7a, ADR-0066): WAV (`hound`) and Ogg Vorbis (`lewton`) decoding, the `sound` and `cue` importers, an own 48 kHz stereo `f32` mixer (buses, voices, stealing, streaming, equal-power spatialization) fed through lock-free `rtrb` rings, and an offline backend whose renders are bit-identical. Audio is presentation-only and never touches a Terra world. `#![forbid(unsafe_code)]`. 2.7a.1 is the library; 2.7a.2 adds the `audio` engine module, the `cpal` backend, the tools and the Studio roles.

## Public entry points
- **Decoding:** `decode_wav`, `decode_ogg`, `OggStream` (packet by packet; exact length from the last granule position); `SoundData` (rate, `Channels`, interleaved samples).
- **Assets:** `SoundImporter` (`sound`: `wav`, `ogg`; `SoundSettings { stream, gain_db, loop_start, loop_end }`; Ogg over `STREAM_AFTER_SECONDS` streams) → `Sound` (`SoundBody::Decoded` or `Streamed`) as `AssetKind::Sound`; `LoadedSound` / `LoadedBody` (shared, ready to play).
- **Cues:** `Cue` (`.cue.ron`: weighted `CueSound`s, `volume_db` and `pitch_semitones` `Span`s, `bus`, `priority`, `looping`, `max_instances`, `spatial`, `seed`), `read_cue` / `write_cue`, `Cue::validate` (`CueError::Field` names the field), `Cue::draw(DrawKey)` → `Draw`; `CueImporter` (`cue`) → `AssetKind::Cue`.
- **Buses:** `BusLayout` / `BusDef` (`.buses.ron`; `read_buses` / `write_buses`), `BusGraph::new` (validated; `index`, `parent`, `names`), `BusIndex`, `MASTER`.
- **Mixing:** `Mixer::new(MixerConfig)` → (`Mixer`, `MixerHandle`). `Mixer::render(&mut [f32])` is the audio-thread half. `MixerHandle` has `play(PlayRequest) → VoiceId`, `stop`, `stop_all`, `set_gains`, `set_bus`, `pump_streams`, `collect` and `stats` (`MixerStats`: steals, drops, underruns, active, per-bus peaks).
- **Math:** `db_to_gain`, `gain_to_db`, `semitones_to_ratio`, `equal_power`, `distance_gain`, `Spatial`, `Listener`, `spatial_gains`.
- **Offline:** `OfflineBackend` (`handle`, `render_frames`, `render_tick`, `stats` → `RenderStats { frames, peak, rms, hash }`, `start_capture` / `take_capture`) and `tick_frames`.

## Invariants
- **`Mixer::render` never allocates, frees, locks or blocks** (`audio_callback_never_allocates`). Voices are built and boxed by `MixerHandle::play`. Finished, stolen and dropped voices go back through the retired ring and are dropped in `MixerHandle::collect`, on the engine thread.
- **Unpositioned voices aren't panned:** mono goes to both channels unchanged and stereo plays as is. A `spatial` stereo voice is averaged to mono before its gains. Gains are never negative.
- **Stealing:** a free slot, else the lowest priority, oldest first, if the new voice outranks or ties it; otherwise the request is dropped (counted). `max_instances` ramps out the group's oldest counting voice.
- **Stops are a 240-frame linear ramp:** frame k is (240 − k)/240.
- **Buses sum children-first** (deepest first, then by index), Master last. Master always exists; a bus with no send goes to Master; a cycle, an unknown send, a duplicate or Master with a send is an error naming the bus.
- **Streams play exactly their decoded twin's frames:** the feeder loops by re-opening the reader and skipping to `loop_start`; the final page's granule position trims the last block. Offline, streams are refilled before every block, so they never underrun.
- **Offline ticks are sample-exact:** tick *t* renders [⌊t·rate/tick_rate⌋, ⌊(t+1)·rate/tick_rate⌋); commands queued before it apply at its first frame.
- **Cue draws** use only `seed ⊕ entity ⊕ (tick × 2⁸) ⊕ n` and SplitMix64 (the sound, then the volume, then the pitch): no clock, thread or world RNG.

## How to test
`cargo nextest run -p concordia-audio`. Every test header derives its expected values by hand, using DC and ramp sources:
- `tests/math.rs`: dB, pan, distance, listener, tick frames;
- `tests/decode.rs`: WAV written in the test with `hound`; `tests/fixtures/audio/tone-440.ogg` from `tools/fixtures/make_audio.sh`;
- `tests/import.rs`: `AssetDb` and the DDC;
- `tests/cue.rs`: SplitMix64's published outputs;
- `tests/mixer.rs`: sums, buses, interpolation, loops, ramps, stealing, groups, spatial, validation;
- `tests/stream.rs`: streamed and decoded renders are equal;
- `tests/offline.rs`: ticks, stats, bit-identical renders;
- `tests/no_alloc.rs`: `allocation-counter`, dev-only (A2).

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
- **DSP:** decibels as 20·log₁₀ of an amplitude ratio; the equal-power cos/sin pan law (Roads, *The Computer Music Tutorial*, 1996); inverse-distance-clamped attenuation (the OpenAL 1.1 specification); linear interpolation.
- **SplitMix64:** Steele, Lea and Flood, 2014; its published outputs for seed 0.
- **Godot (MIT):** the `AudioStreamPlayer` and audio-bus docs, for semantics only.
- No Unreal, Wwise, FMOD or other non-open source was consulted.

## Status
M2 phase 2.7a.1 (library). 2.7a.2 adds the engine module, components and signals, `cpal`, the `audio.*` tools and the Studio roles.

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
