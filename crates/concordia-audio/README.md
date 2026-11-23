# concordia-audio

Concordia's audio: WAV and Ogg Vorbis decoding (`hound`, `lewton`), the `sound` and `cue` importers, `.cue.ron` cues with seeded variation, `.buses.ron` bus layouts, and an own 48 kHz mixer with voices, stealing, streaming and equal-power spatialization. It is fed through lock-free rings and never allocates on the audio thread. The offline backend renders sample-exact, bit-identical buffers for headless tests and CI. Audio is presentation-only (ADR-0066).

See `AGENTS.md` for invariants, tests and references.
