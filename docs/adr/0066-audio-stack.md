# ADR-0066: Audio stack: cpal, lewton and hound; an own mixer outside the world; `.cue.ron` cues
- Status: Accepted (M2 phase 2.7a, 2026-11-22; author decisions A1 and A2). The mixer, decoders, cues and offline backend landed in 2.7a.1; the engine module, `cpal` and the tools follow in 2.7a.2.
- Date: 2026-11-22
- Builds on: ADR-0044 (asset ids and the DDC), ADR-0063 (determinism contract), ADR-0069 (storage kinds, commands, signals), ADR-0073 (per-role tool budgets), ADR-0076 (the `#[reflect(presentation)]` precedent)

## Context
M2's sample game and later ports need sound through buses, spatial cues and streamed music, played from gameplay, scripts and animation events. Audio is presentation: it must never change a world hash, at any thread count or through a replay, and headless tests and CI have no audio device. The engine's licence policy is permissive-only; `symphonia`, the usual pure-Rust decoder set, is MPL-2.0.

## Decision
1. **The stack.**
   - **Output:** `cpal` (Apache-2.0). On macOS it goes through `coreaudio-rs` over `objc2`, so it needs no bindgen.
   - **Decoding:** `lewton` (MIT/Apache) for Ogg Vorbis and `hound` (Apache-2.0) for WAV. No MPL decoder is used in the runtime.
   - **Queues:** `rtrb` (MIT/Apache) wait-free single-producer, single-consumer rings.
   - **Testing:** a test-only allocation counter, `allocation-counter` (A2), so the engine gains no `unsafe`.
   - `concordia-audio` is `#![forbid(unsafe_code)]`.
2. **An own mixer at 48 kHz, stereo `f32`.**
   - **Voices:** a fixed pool of 1–256, resampled by linear interpolation at `source_rate / 48000 × pitch`.
   - **Stealing:** a new voice takes the lowest-priority, oldest voice it outranks or ties, or is dropped.
   - **Limits:** `max_instances` ramps out the oldest voices of a group.
   - **Stopping:** a stop is a 240-frame (5 ms) linear ramp.
   - **Buses:** a tree under Master (default SFX → Master and Music → Master; a `.buses.ron` file overrides it), summed children-first with dB gains and mutes.
   - **Spatialization:** inverse-distance-clamped attenuation (OpenAL 1.1's model) and equal-power panning from the listener-space azimuth. A positioned stereo source is averaged to mono; unpositioned voices aren't panned.
3. **The audio thread never allocates, frees, locks or blocks.**
   - Voices are built on the engine side and boxed there.
   - Commands cross in one ring, and finished or stolen voices come back through a second ring, so their `Arc`s and stream rings are dropped on the engine thread.
   - A test measures `Mixer::render` at zero allocations and zero frees.
4. **Streaming.**
   - A streamed (Ogg) voice reads a 2 s ring that an engine-side feeder fills.
   - Loops are made by re-opening the reader and skipping to `loop_start`, so a streamed sound plays exactly the frames of its decoded twin.
   - Lengths are exact: the final page's granule position trims the last Vorbis block, so loops are seamless.
   - An underrun plays silence and is counted.
5. **Two backends.**
   - **`offline`** renders exactly the frames each tick covers, [⌊t·48000/rate⌋, ⌊(t+1)·48000/rate⌋), with commands applied at the tick's first frame. It refills streams before every block and keeps a running BLAKE3 hash, peak and RMS, so its renders are bit-identical.
   - **`cpal`** plays live, and is used only by `concordia run`. Headless engines, tests, CI and replays always use `offline`.
   - A missing or failing device is a warning, and the engine falls back to offline.
6. **Cues are engine-native text (`.cue.ron`).**
   - **Fields:** weighted sounds, volume (dB) and pitch (semitone) ranges, a bus, a priority, looping, `max_instances`, an optional `spatial` block and a `seed`.
   - **The draw:** each play draws from SplitMix64's stream seeded with `seed ⊕ entity ⊕ (tick × 2⁸) ⊕ n`: the sound, then the volume, then the pitch. It never uses a clock, a thread or the world RNG.
   - Sounds and cues are imported by the `sound` (`wav`, `ogg`) and `cue` importers into the DDC as `AssetKind::Sound` and `AssetKind::Cue`. Ogg over 10 s streams by default.
7. **The determinism boundary: audio lives outside the world** (2.7a.2).
   - **Hashed:** gameplay's requests are ordinary simulation state, so the `AudioEmitter` and `AudioListener` components, `AnimEventCues` and pending `PlayCue` / `StopCue` signals are hashed.
   - **Not in the world at all:** voices, the mixer, runtime bus gains, the emitter-to-voice map, draws, streams, previews and the offline capture.
   - **`audio.collect`** runs in `Phase::Presentation`. It reads and receives only, writes nothing, and pushes commands into a queue it owns. That is stronger than `#[reflect(presentation)]` (ADR-0076's precedent): there is nothing to skip. Terra's planner already rejects a presentation processor that writes simulation state.
   - **The proof:** the same run with and without `audio.collect` hashes identically at every tick, at 1 and 8 threads and through a replay.

## Alternatives
- **`symphonia`:** MPL-2.0, which `cargo-deny` rejects in the engine.
- **`rodio` or `kira`:** each brings its own mixer and timing model. Neither renders sample-exact offline at tick boundaries, nor keeps state out of the audio thread's allocator.
- **Audio state as `#[reflect(presentation)]` components:** this would be inspectable through `query.run`, but it would put per-voice state in the world, which only presentation processors could write. `audio.inspect` gives the same view with nothing in the world.
- **Godot sound banks and bus layouts as the formats:** these are M5's import (ADR-0067, D1), which maps onto `.cue.ron` and `.buses.ron`.
- **A per-thread or wall-clock RNG for variation:** offline renders would no longer be bit-identical.
- **Our own counting allocator:** about ten lines of `unsafe`, so the test-only crate was chosen (A2).

## Consequences
- New crate `concordia-audio`; new formats `.cue.ron` and `.buses.ron`; new asset kinds `Sound` and `Cue`; workspace dependencies `hound`, `lewton`, `rtrb` and the dev-only `allocation-counter`. 2.7a.2 adds `cpal`.
- Signals raised on tick *t* sound on tick *t* + 1 (about 17 ms at 60 Hz).
- Studio (A1, 2.7a.2): the sound designer and composer are enabled; the synthesizer (`gen_run`) stays M3 3.7.
- DSP effects, HRTF, Doppler, occlusion, crossfades and UI sounds are later work.
