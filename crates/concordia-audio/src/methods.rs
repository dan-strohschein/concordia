//! `audio.play`, `audio.stop`, `audio.buses`, `audio.render` and `audio.inspect`. None writes
//! the world, so none needs an actor.

use std::sync::Arc;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method, MethodInfo};
use concordia_assets::{AssetDb, AssetRef};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::core::{AudioShared, Start};
use crate::cue::{Cue, DrawKey};
use crate::gain::gain_to_db;
use crate::mixer::VoiceId;

/// Every audio method.
pub fn all() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<AudioPlay>(),
        MethodInfo::of::<AudioStop>(),
        MethodInfo::of::<AudioBuses>(),
        MethodInfo::of::<AudioRender>(),
        MethodInfo::of::<AudioInspect>(),
    ]
}

fn invalid(message: impl Into<String>, at: &str) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message).at(at)
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ApiErrorKind::Internal, e.to_string())
}

/// The engine's audio state (`[audio]` is on whenever these methods exist).
fn shared(engine: &Engine) -> Result<AudioShared, ApiError> {
    engine
        .state::<AudioShared>()
        .cloned()
        .ok_or_else(|| internal("the audio module's state is missing"))
}

/// Loads `cue` (and its sounds) into the engine's library from the asset database.
fn ensure_cue(engine: &mut Engine, cue: &AssetRef) -> Result<(AudioShared, Arc<Cue>), ApiError> {
    let shared = shared(engine)?;
    let found = shared.lock().library().cue(cue).cloned();
    if let Some(found) = found {
        return Ok((shared, found));
    }
    let db = engine
        .state_mut::<AssetDb>()
        .ok_or_else(|| invalid("cues load through the asset database; add [assets]", "/cue"))?;
    let mut library = shared.lock().library.clone();
    let loaded = library.ensure(db, cue).map_err(|e| invalid(e, "/cue"))?;
    shared.lock().library = library;
    Ok((shared, loaded))
}

/// Preview a cue outside the world (it never touches a world hash): it plays on the engine's
/// output, positioned relative to the listener if `at` is given. Headless engines mix offline,
/// so listen with `audio.render` instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioPlay;

/// `audio.play` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AudioPlayParams {
    /// The `.cue.ron` cue.
    pub cue: AssetRef,
    /// Extra volume in decibels (default 0).
    #[serde(default)]
    pub volume_db: Option<f32>,
    /// Where it sounds (default: unpositioned).
    #[serde(default)]
    pub at: Option<Vec3>,
}

/// `audio.play` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioPlayResult {
    /// The voice, for `audio.stop`.
    pub voice: u64,
}

impl Method for AudioPlay {
    const NAME: &'static str = "audio.play";
    const MUTATES: bool = false;
    type Params = AudioPlayParams;
    type Result = AudioPlayResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: AudioPlayParams,
    ) -> Result<AudioPlayResult, ApiError> {
        let volume_db = p.volume_db.unwrap_or(0.0);
        if !volume_db.is_finite() || !(-80.0..=24.0).contains(&volume_db) {
            return Err(invalid(
                format!("volume_db {volume_db} must be in [-80, 24]"),
                "/volume_db",
            ));
        }
        let (shared, _) = ensure_cue(engine, &p.cue)?;
        let mut core = shared.lock();
        core.previews += 1;
        let key = DrawKey {
            entity: 0,
            tick: core.previews,
            n: 0,
        };
        let start = Start {
            cue: &p.cue,
            entity: None,
            position: p.at,
            volume_db,
            key,
        };
        let voice = core
            .start(start)
            .map_err(|e| invalid(e.to_string(), "/cue"))?;
        Ok(AudioPlayResult { voice: voice.0 })
    }
}

/// Stop one voice, or every voice (each with a 5 ms ramp).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioStop;

/// `audio.stop` params.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct AudioStopParams {
    /// The voice (default: all).
    pub voice: Option<u64>,
}

/// `audio.stop` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioStopResult {
    /// Voices asked to stop.
    pub stopped: u32,
}

impl Method for AudioStop {
    const NAME: &'static str = "audio.stop";
    const MUTATES: bool = false;
    type Params = AudioStopParams;
    type Result = AudioStopResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: AudioStopParams,
    ) -> Result<AudioStopResult, ApiError> {
        let shared = shared(engine)?;
        let mut core = shared.lock();
        core.prune();
        let voices: Vec<VoiceId> = match p.voice {
            Some(v) => core
                .playing()
                .contains_key(&VoiceId(v))
                .then_some(VoiceId(v))
                .into_iter()
                .collect(),
            None => core.playing().keys().copied().collect(),
        };
        for voice in &voices {
            core.stop(*voice);
        }
        Ok(AudioStopResult {
            stopped: voices.len() as u32,
        })
    }
}

/// List the buses (send, gain, mute and peak level since the last call), optionally changing
/// gains and mutes first. Changes last until the engine closes; the `.buses.ron` file named by
/// `[audio] buses` is the saved mix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioBuses;

/// One bus change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BusChange {
    /// The bus.
    pub name: String,
    /// New gain in decibels, in [−80, 24].
    #[serde(default)]
    pub gain_db: Option<f32>,
    /// New mute.
    #[serde(default)]
    pub mute: Option<bool>,
}

/// `audio.buses` params.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct AudioBusesParams {
    /// Changes to apply first.
    pub set: Vec<BusChange>,
}

/// One bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct BusReport {
    /// Its name.
    pub name: String,
    /// Its parent (none for Master).
    pub send: Option<String>,
    /// Gain in decibels.
    pub gain_db: f32,
    /// Silenced.
    pub mute: bool,
    /// Peak level since the last `audio.buses`, in dBFS (−80 is silence).
    pub peak_dbfs: f32,
}

/// `audio.buses` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioBusesResult {
    /// Master first.
    pub buses: Vec<BusReport>,
}

impl Method for AudioBuses {
    const NAME: &'static str = "audio.buses";
    const MUTATES: bool = false;
    type Params = AudioBusesParams;
    type Result = AudioBusesResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: AudioBusesParams,
    ) -> Result<AudioBusesResult, ApiError> {
        let shared = shared(engine)?;
        let mut core = shared.lock();
        for (i, change) in p.set.iter().enumerate() {
            let at = format!("/set/{i}");
            let bus = core.buses().index(&change.name).ok_or_else(|| {
                invalid(format!("no bus {:?}", change.name), &format!("{at}/name"))
            })?;
            if let Some(db) = change.gain_db
                && (!db.is_finite() || !(-80.0..=24.0).contains(&db))
            {
                return Err(invalid(
                    format!("gain_db {db} must be in [-80, 24]"),
                    &format!("{at}/gain_db"),
                ));
            }
            core.set_bus(bus, change.gain_db, change.mute)
                .map_err(internal)?;
        }
        Ok(AudioBusesResult {
            buses: core.bus_reports(),
        })
    }
}

/// Render offline and measure: a cue alone through the bus layout for `seconds`, or (without
/// a cue) the play world's audio so far. Reports frames, peak and RMS in dBFS and a BLAKE3
/// hash (equal hashes are bit-identical audio); `path` also writes a 16-bit WAV (the newest
/// 30 s for the play world).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioRender;

/// `audio.render` params.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct AudioRenderParams {
    /// The cue (default: the play world's render).
    pub cue: Option<AssetRef>,
    /// Seconds to render a cue, in (0, 600] (default 2).
    pub seconds: Option<f32>,
    /// A project-relative `.wav` to write.
    pub path: Option<String>,
}

/// `audio.render` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioRenderResult {
    /// Frames rendered (48 kHz).
    pub frames: u64,
    /// Peak level in dBFS (−80 is silence).
    pub peak_dbfs: f32,
    /// RMS level in dBFS.
    pub rms_dbfs: f32,
    /// BLAKE3 of the samples, hex.
    pub hash: String,
    /// The WAV written, if any.
    pub path: Option<String>,
}

impl Method for AudioRender {
    const NAME: &'static str = "audio.render";
    const MUTATES: bool = false;
    type Params = AudioRenderParams;
    type Result = AudioRenderResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: AudioRenderParams,
    ) -> Result<AudioRenderResult, ApiError> {
        let path = p
            .path
            .as_deref()
            .map(|path| engine.project_file(path, ".wav"))
            .transpose()?;
        let (stats, samples) = match &p.cue {
            Some(cue) => {
                let seconds = p.seconds.unwrap_or(2.0);
                if !seconds.is_finite() || seconds <= 0.0 || seconds > 600.0 {
                    return Err(invalid(
                        format!("seconds {seconds} must be in (0, 600]"),
                        "/seconds",
                    ));
                }
                let (shared, _) = ensure_cue(engine, cue)?;
                let core = shared.lock();
                core.render_cue(cue, seconds)
                    .map_err(|e| invalid(e.to_string(), "/cue"))?
            }
            None => {
                let shared = shared(engine)?;
                let mut core = shared.lock();
                let Some(stats) = core.render_stats() else {
                    let backend = core.backend_name();
                    return Err(invalid(
                        format!("the {backend} backend keeps no render; pass a cue"),
                        "/cue",
                    ));
                };
                (stats, core.capture())
            }
        };
        if let Some(path) = &path {
            crate::wav::write_wav16(path, &samples).map_err(|e| invalid(e, "/path"))?;
        }
        Ok(AudioRenderResult {
            frames: stats.frames,
            peak_dbfs: gain_to_db(stats.peak),
            rms_dbfs: gain_to_db(stats.rms),
            hash: stats.hash,
            path: p.path,
        })
    }
}

/// What is playing: the output, every voice (cue, entity, bus, level, pan and position), the
/// listener, and the steal, drop and underrun counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioInspect;

/// `audio.inspect` params (none).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct AudioInspectParams {}

/// One voice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct VoiceReport {
    /// Its id.
    pub voice: u64,
    /// The cue.
    pub cue: AssetRef,
    /// The entity that started it (none for previews).
    pub entity: Option<EntityId>,
    /// Its bus.
    pub bus: String,
    /// Volume in decibels.
    pub volume_db: f32,
    /// `[left, right]` gains.
    pub gains: [f32; 2],
    /// Where it is (positioned voices).
    pub position: Option<Vec3>,
}

/// The listener.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ListenerReport {
    /// Its entity.
    pub entity: EntityId,
    /// Where it is.
    pub position: Vec3,
    /// Which way it faces.
    pub forward: Vec3,
}

/// `audio.inspect` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AudioInspectResult {
    /// `"offline"`, `"device"` or `"none"`.
    pub backend: String,
    /// Voices, by id.
    pub voices: Vec<VoiceReport>,
    /// The listener, if any.
    pub listener: Option<ListenerReport>,
    /// Cues loaded.
    pub cues: u32,
    /// Voices stolen for new ones.
    pub steals: u64,
    /// Requests dropped (every voice outranked them).
    pub drops: u64,
    /// Stream samples that weren't ready in time.
    pub underruns: u64,
    /// Live streams.
    pub streams: u32,
}

impl Method for AudioInspect {
    const NAME: &'static str = "audio.inspect";
    const MUTATES: bool = false;
    type Params = AudioInspectParams;
    type Result = AudioInspectResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        _: AudioInspectParams,
    ) -> Result<AudioInspectResult, ApiError> {
        let shared = shared(engine)?;
        let mut core = shared.lock();
        core.prune();
        let voices = core
            .playing()
            .iter()
            .map(|(voice, info)| VoiceReport {
                voice: voice.0,
                cue: info.cue.clone(),
                entity: info.entity,
                bus: info.bus.clone(),
                volume_db: gain_to_db(info.volume),
                gains: info.gains,
                position: info.position,
            })
            .collect();
        let listener = core.listener().map(|(entity, l)| ListenerReport {
            entity,
            position: l.position,
            forward: l.forward,
        });
        let cues = core.library().cues() as u32;
        let backend = core.backend_name().to_owned();
        let (steals, drops, underruns, streams) = core.handle().map_or((0, 0, 0, 0), |h| {
            let s = h.stats();
            (s.steals(), s.drops(), s.underruns(), h.streams() as u32)
        });
        Ok(AudioInspectResult {
            backend,
            voices,
            listener,
            cues,
            steals,
            drops,
            underruns,
            streams,
        })
    }
}
