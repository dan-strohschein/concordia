//! Concordia audio (M2 phase 2.7a, ADR-0066): WAV (hound) and Ogg Vorbis (lewton) decoding, the
//! `sound` and `cue` importers, an own 48 kHz stereo `f32` mixer with buses, voices, stealing,
//! streaming and equal-power spatialization, fed through lock-free `rtrb` rings, and an offline
//! backend whose renders are bit-identical. Audio is presentation-only: nothing here touches a
//! Terra world.
#![forbid(unsafe_code)]

mod bus;
mod cue;
mod decode;
mod error;
mod gain;
mod import;
mod mixer;
mod offline;
mod sound;
mod spatial;
mod stream;
mod voice;

pub use bus::{BusDef, BusGraph, BusIndex, BusLayout, MASTER, read_buses, write_buses};
pub use cue::{Cue, CueImporter, CueSound, Draw, DrawKey, Span, read_cue, write_cue};
pub use decode::{OggStream, decode_ogg, decode_wav};
pub use error::{AudioError, CueError};
pub use gain::{SILENCE_DB, db_to_gain, gain_to_db, semitones_to_ratio};
pub use import::{STREAM_AFTER_SECONDS, SoundImporter, SoundSettings};
pub use mixer::{
    COMMAND_CAPACITY, MAX_VOICES, Mixer, MixerConfig, MixerHandle, MixerStats, PlayRequest,
    SAMPLE_RATE, STOP_RAMP_FRAMES, VoiceId,
};
pub use offline::{OfflineBackend, RenderStats, tick_frames};
pub use sound::{Channels, LoadedBody, LoadedSound, Sound, SoundBody, SoundData};
pub use spatial::{Listener, Spatial, distance_gain, equal_power, spatial_gains};
