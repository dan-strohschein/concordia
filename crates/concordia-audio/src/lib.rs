//! Concordia audio (M2 phase 2.7a, ADR-0066): WAV (hound) and Ogg Vorbis (lewton) decoding, the
//! `sound` and `cue` importers, an own 48 kHz stereo `f32` mixer with buses, voices, stealing,
//! streaming and equal-power spatialization, fed through lock-free `rtrb` rings, and an offline
//! backend whose renders are bit-identical. Audio is presentation-only: nothing here touches a
//! Terra world.
#![forbid(unsafe_code)]

mod bus;
mod collect;
mod components;
mod core;
mod cue;
mod debug;
mod decode;
mod device;
mod error;
mod gain;
mod import;
mod library;
mod methods;
mod mixer;
mod module;
mod offline;
mod sound;
mod spatial;
mod stream;
mod voice;
mod wav;

pub use bus::{BusDef, BusGraph, BusIndex, BusLayout, MASTER, read_buses, write_buses};
pub use collect::{COLLECT, Collector};
pub use components::{
    AnimEventCues, AudioEmitter, AudioListener, PlayCue, StopCue, register_types,
};
pub use core::{
    AudioCore, AudioDevice, AudioSettings, AudioShared, Backend, CAPTURE_SECONDS, OpenDevice,
    VoiceInfo,
};
pub use cue::{Cue, CueImporter, CueSound, Draw, DrawKey, Span, read_cue, write_cue};
pub use debug::{CIRCLE_SEGMENTS, geometry as debug_geometry};
pub use decode::{OggStream, decode_ogg, decode_wav};
pub use device::CpalDevice;
pub use error::{AudioError, CueError};
pub use gain::{SILENCE_DB, db_to_gain, gain_to_db, semitones_to_ratio};
pub use import::{STREAM_AFTER_SECONDS, SoundImporter, SoundSettings};
pub use library::AudioLibrary;
pub use methods::{
    AudioBuses, AudioBusesParams, AudioBusesResult, AudioInspect, AudioInspectParams,
    AudioInspectResult, AudioPlay, AudioPlayParams, AudioPlayResult, AudioRender,
    AudioRenderParams, AudioRenderResult, AudioStop, AudioStopParams, AudioStopResult, BusChange,
    BusReport, ListenerReport, VoiceReport,
};
pub use mixer::{
    COMMAND_CAPACITY, MAX_VOICES, Mixer, MixerConfig, MixerHandle, MixerStats, PlayRequest,
    SAMPLE_RATE, STOP_RAMP_FRAMES, VoiceId,
};
pub use module::{AUDIO_LAYER, AudioModule};
pub use offline::{OfflineBackend, RenderStats, tick_frames};
pub use sound::{Channels, LoadedBody, LoadedSound, Sound, SoundBody, SoundData};
pub use spatial::{Listener, Spatial, distance_gain, equal_power, spatial_gains};
pub use wav::write_wav16;

/// The importers every project registers for audio (`sound`: wav, ogg; `cue`: .cue.ron).
pub fn importers() -> Vec<std::sync::Arc<dyn concordia_assets::Importer>> {
    vec![
        std::sync::Arc::new(SoundImporter),
        std::sync::Arc::new(CueImporter),
    ]
}
