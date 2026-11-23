//! The engine's audio state, shared by the presentation processor, the methods and the debug
//! layer: the output (offline, a device, or none), the cue library and what each voice is.
//! None of it is in the world, so none of it is hashed (ADR-0066).

use std::any::Any;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard};

use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;

use std::num::NonZeroU16;

use crate::bus::{BusGraph, BusIndex, BusLayout};
use crate::cue::{Cue, DrawKey};
use crate::error::AudioError;
use crate::gain::db_to_gain;
use crate::library::AudioLibrary;
use crate::mixer::PlayRequest;
use crate::mixer::{MixerConfig, MixerHandle, SAMPLE_RATE, VoiceId};
use crate::offline::{OfflineBackend, RenderStats};
use crate::spatial::Listener;
use crate::spatial::spatial_gains;

/// How long the play world's offline capture reaches back.
pub const CAPTURE_SECONDS: usize = 30;

/// An audio output device (cpal, or a test double).
pub trait AudioDevice: Send + Sync + std::fmt::Debug {
    /// Opens the device and starts a mixer on it; it plays until the guard is dropped.
    fn open(&self, buses: &BusGraph, voices: NonZeroUsize) -> Result<OpenDevice, AudioError>;
}

/// An opened device: the engine's half of its mixer, and what keeps it playing.
pub struct OpenDevice {
    /// Queues commands to the device's mixer.
    pub handle: MixerHandle,
    /// Dropping it closes the device.
    pub guard: Box<dyn Any + Send>,
}

/// Where the mix goes.
#[derive(Debug, Clone)]
pub enum Backend {
    /// Rendered per tick, sample-exact (headless engines, tests, CI, replays).
    Offline,
    /// A live device; failing to open it falls back to offline with a warning.
    Device(Arc<dyn AudioDevice>),
    /// No mixing at all (`[audio] backend = "none"`).
    None,
}

/// The `audio` module's settings (`[audio]`).
#[derive(Debug, Clone)]
pub struct AudioSettings {
    /// The voice pool (1–256).
    pub voices: NonZeroUsize,
    /// The bus layout.
    pub buses: BusLayout,
    /// The output.
    pub backend: Backend,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            voices: NonZeroUsize::new(32).expect("non-zero"),
            buses: BusLayout::default(),
            backend: Backend::Offline,
        }
    }
}

/// The mix's destination, as opened.
pub(crate) enum Output {
    Offline(Box<OfflineBackend>),
    Device(Box<DeviceOutput>),
    None,
}

/// An opened device's handle and guard.
pub(crate) struct DeviceOutput {
    handle: MixerHandle,
    _guard: Box<dyn Any + Send>,
}

/// What a voice is, for `audio.inspect` and the debug layer.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceInfo {
    /// The cue it plays.
    pub cue: AssetRef,
    /// The entity that started it (none for previews).
    pub entity: Option<EntityId>,
    /// Its bus.
    pub bus: String,
    /// Linear volume.
    pub volume: f32,
    /// `[left, right]` gains.
    pub gains: [f32; 2],
    /// Where it is (positioned voices).
    pub position: Option<Vec3>,
}

/// The engine's audio state.
pub struct AudioCore {
    pub(crate) output: Output,
    pub(crate) buses: BusGraph,
    pub(crate) voices: NonZeroUsize,
    pub(crate) library: AudioLibrary,
    pub(crate) playing: BTreeMap<VoiceId, VoiceInfo>,
    pub(crate) listener: Option<(EntityId, Listener)>,
    pub(crate) listener_gain: f32,
    pub(crate) base_tick: Option<u64>,
    pub(crate) previews: u64,
    pub(crate) bus_gain_db: Vec<f32>,
    pub(crate) bus_mute: Vec<bool>,
}

/// What to start.
pub(crate) struct Start<'a> {
    pub(crate) cue: &'a AssetRef,
    pub(crate) entity: Option<EntityId>,
    pub(crate) position: Option<Vec3>,
    pub(crate) volume_db: f32,
    pub(crate) key: DrawKey,
}

/// A cue's instance-limit group: the first 8 bytes of BLAKE3 of its reference.
fn group_key(cue: &AssetRef) -> u64 {
    let hash = blake3::hash(cue.to_string().as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

impl std::fmt::Debug for AudioCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioCore")
            .field("backend", &self.backend_name())
            .field("voices", &self.playing.len())
            .finish()
    }
}

impl AudioCore {
    /// Opens the output `settings` asks for; a device that fails is a warning, and offline.
    pub fn new(settings: &AudioSettings) -> Result<Self, AudioError> {
        let buses = BusGraph::new(&settings.buses)?;
        let output = match &settings.backend {
            Backend::None => Output::None,
            Backend::Offline => Output::Offline(Box::new(offline(&buses, settings.voices)?)),
            Backend::Device(device) => match device.open(&buses, settings.voices) {
                Ok(open) => Output::Device(Box::new(DeviceOutput {
                    handle: open.handle,
                    _guard: open.guard,
                })),
                Err(e) => {
                    tracing::warn!(target: "concordia::audio", error = %e, "no audio device; mixing offline instead");
                    Output::Offline(Box::new(offline(&buses, settings.voices)?))
                }
            },
        };
        Ok(Self {
            output,
            voices: settings.voices,
            library: AudioLibrary::default(),
            playing: BTreeMap::new(),
            listener: None,
            listener_gain: 1.0,
            base_tick: None,
            previews: 0,
            bus_gain_db: (0..buses.len())
                .map(|b| buses.gain_db(BusIndex(b)))
                .collect(),
            bus_mute: (0..buses.len()).map(|b| buses.muted(BusIndex(b))).collect(),
            buses,
        })
    }

    /// `"offline"`, `"device"` or `"none"`.
    pub fn backend_name(&self) -> &'static str {
        match self.output {
            Output::Offline(_) => "offline",
            Output::Device(_) => "device",
            Output::None => "none",
        }
    }

    /// The mixer's engine half (none when mixing is off).
    pub fn handle(&mut self) -> Option<&mut MixerHandle> {
        match &mut self.output {
            Output::Offline(b) => Some(b.handle()),
            Output::Device(d) => Some(&mut d.handle),
            Output::None => None,
        }
    }

    /// The bus layout.
    pub fn buses(&self) -> &BusGraph {
        &self.buses
    }

    /// The loaded cues and sounds.
    pub fn library(&self) -> &AudioLibrary {
        &self.library
    }

    /// The voices playing, by id.
    pub fn playing(&self) -> &BTreeMap<VoiceId, VoiceInfo> {
        &self.playing
    }

    /// The listener (its entity and pose), if any.
    pub fn listener(&self) -> Option<(EntityId, Listener)> {
        self.listener
    }

    /// The offline render's statistics since play started (none on a device or with no mixing).
    pub fn render_stats(&self) -> Option<RenderStats> {
        match &self.output {
            Output::Offline(b) => Some(b.stats()),
            _ => None,
        }
    }

    /// The newest `CAPTURE_SECONDS` of the offline render (interleaved stereo, 48 kHz).
    pub fn capture(&mut self) -> Vec<f32> {
        match &mut self.output {
            Output::Offline(b) => b.captured(),
            _ => Vec::new(),
        }
    }

    /// A new play session: an empty offline render, no voices, the library given.
    pub(crate) fn start_play(&mut self, library: AudioLibrary) -> Result<(), AudioError> {
        self.stop_all();
        self.library = library;
        self.base_tick = None;
        self.listener = None;
        if let Output::Offline(_) = self.output {
            self.output = Output::Offline(Box::new(offline(&self.buses, self.voices)?));
            // Runtime bus changes last until the engine closes, across plays.
            for b in 0..self.buses.len() {
                self.set_bus(BusIndex(b), None, None)?;
            }
        }
        Ok(())
    }

    /// Stops every voice (with the ramp) and forgets them.
    pub fn stop_all(&mut self) {
        if let Some(h) = self.handle() {
            let _ = h.stop_all();
        }
        self.playing.clear();
    }

    /// Starts `cue` (drawn with `key`): positioned when the cue is spatial and `position` is
    /// given, relative to the current listener.
    pub(crate) fn start(&mut self, start: Start<'_>) -> Result<VoiceId, AudioError> {
        let cue = self
            .library
            .cue(start.cue)
            .cloned()
            .ok_or_else(|| AudioError::Request(format!("cue {} is not loaded", start.cue)))?;
        let draw = cue.draw(start.key);
        let entry = &cue.sounds[draw.sound.min(cue.sounds.len() - 1)];
        let sound =
            self.library.sound(&entry.sound).cloned().ok_or_else(|| {
                AudioError::Request(format!("sound {} is not loaded", entry.sound))
            })?;
        let bus = self.buses.index(&cue.bus).ok_or_else(|| {
            AudioError::Request(format!("cue {}: no bus {:?}", start.cue, cue.bus))
        })?;
        let position = cue.spatial.and(start.position);
        let gains = position.map_or([1.0, 1.0], |p| self.gains_at(p, &cue));
        let volume = db_to_gain(draw.volume_db + start.volume_db);
        let request = PlayRequest {
            volume,
            pitch: draw.pitch,
            priority: cue.priority,
            looping: cue.looping,
            spatial: position.is_some(),
            gains,
            group: cue
                .max_instances
                .and_then(NonZeroU16::new)
                .map(|max| (group_key(start.cue), max)),
            ..PlayRequest::new(sound, bus)
        };
        let handle = self.handle().ok_or_else(|| {
            AudioError::Request("mixing is off ([audio] backend = \"none\")".into())
        })?;
        let voice = handle.play(request)?;
        self.playing.insert(
            voice,
            VoiceInfo {
                cue: start.cue.clone(),
                entity: start.entity,
                bus: cue.bus.clone(),
                volume,
                gains,
                position,
            },
        );
        Ok(voice)
    }

    /// `[left, right]` for a positioned voice of `cue` at `position`.
    fn gains_at(&self, position: Vec3, cue: &Cue) -> [f32; 2] {
        let spatial = cue.spatial.unwrap_or_default();
        let listener = self.listener.map(|(_, l)| l);
        spatial_gains(listener.as_ref(), position, &spatial).map(|g| g * self.listener_gain)
    }

    /// Moves a positioned voice (and re-pans it).
    pub(crate) fn move_voice(&mut self, voice: VoiceId, position: Vec3) {
        let Some(info) = self.playing.get(&voice) else {
            return;
        };
        let Some(cue) = self.library.cue(&info.cue).cloned() else {
            return;
        };
        let gains = self.gains_at(position, &cue);
        if let Some(info) = self.playing.get_mut(&voice) {
            info.position = Some(position);
            info.gains = gains;
        }
        if let Some(h) = self.handle() {
            let _ = h.set_gains(voice, gains);
        }
    }

    /// Stops one voice with the ramp.
    pub(crate) fn stop(&mut self, voice: VoiceId) {
        if let Some(h) = self.handle() {
            let _ = h.stop(voice);
        }
        self.playing.remove(&voice);
    }

    /// Changes a bus's runtime gain and mute (unchanged parts stay).
    pub(crate) fn set_bus(
        &mut self,
        bus: BusIndex,
        gain_db: Option<f32>,
        mute: Option<bool>,
    ) -> Result<(), AudioError> {
        let i = bus.get();
        if let Some(db) = gain_db {
            self.bus_gain_db[i] = db;
        }
        if let Some(m) = mute {
            self.bus_mute[i] = m;
        }
        let (db, m) = (self.bus_gain_db[i], self.bus_mute[i]);
        match self.handle() {
            Some(h) => h.set_bus(bus, db, m),
            None => Ok(()),
        }
    }

    /// Every bus's runtime state and its peak since the last report.
    pub(crate) fn bus_reports(&mut self) -> Vec<crate::methods::BusReport> {
        let names = self.buses.names().to_vec();
        let peaks: Vec<f32> = match self.handle() {
            Some(h) => (0..names.len())
                .map(|b| h.stats().take_peak(BusIndex(b)))
                .collect(),
            None => vec![0.0; names.len()],
        };
        names
            .iter()
            .enumerate()
            .map(|(b, name)| crate::methods::BusReport {
                name: name.clone(),
                send: self
                    .buses
                    .parent(BusIndex(b))
                    .map(|p| self.buses.names()[p.get()].clone()),
                gain_db: self.bus_gain_db[b],
                mute: self.bus_mute[b],
                peak_dbfs: crate::gain::gain_to_db(peaks[b]),
            })
            .collect()
    }

    /// Renders `cue` alone for `seconds` through a fresh offline mixer with this layout and the
    /// runtime bus settings: its statistics and samples.
    pub(crate) fn render_cue(
        &self,
        cue: &AssetRef,
        seconds: f32,
    ) -> Result<(RenderStats, Vec<f32>), AudioError> {
        let mut core = AudioCore {
            output: Output::Offline(Box::new(offline(&self.buses, self.voices)?)),
            buses: self.buses.clone(),
            voices: self.voices,
            library: self.library.clone(),
            playing: BTreeMap::new(),
            listener: None,
            listener_gain: 1.0,
            base_tick: None,
            previews: 0,
            bus_gain_db: self.bus_gain_db.clone(),
            bus_mute: self.bus_mute.clone(),
        };
        for b in 0..self.buses.len() {
            core.set_bus(BusIndex(b), None, None)?;
        }
        if let Output::Offline(backend) = &mut core.output {
            backend.start_capture();
        }
        core.start(Start {
            cue,
            entity: None,
            position: None,
            volume_db: 0.0,
            key: DrawKey::default(),
        })?;
        let frames = (f64::from(seconds) * f64::from(SAMPLE_RATE)).round() as u64;
        match &mut core.output {
            Output::Offline(backend) => {
                backend.render_frames(frames);
                Ok((backend.stats(), backend.take_capture()))
            }
            _ => unreachable!("built offline above"),
        }
    }

    /// Forgets voices the mixer has retired.
    pub(crate) fn prune(&mut self) {
        let live: Vec<VoiceId> = match self.handle() {
            Some(h) => {
                h.collect();
                h.live().collect()
            }
            None => Vec::new(),
        };
        self.playing.retain(|id, _| live.binary_search(id).is_ok());
    }
}

fn offline(buses: &BusGraph, voices: NonZeroUsize) -> Result<OfflineBackend, AudioError> {
    let mut config = MixerConfig::new(buses.clone());
    config.voices = voices;
    let mut backend = OfflineBackend::new(config)?;
    backend.keep_last(CAPTURE_SECONDS * SAMPLE_RATE as usize);
    Ok(backend)
}

/// The engine's audio state behind a lock (the audio thread never takes it).
#[derive(Debug, Clone)]
pub struct AudioShared(Arc<Mutex<AudioCore>>);

impl AudioShared {
    /// Shares `core`.
    pub fn new(core: AudioCore) -> Self {
        Self(Arc::new(Mutex::new(core)))
    }

    /// Locks it (a poisoned lock is recovered: audio state is never left half-written).
    pub fn lock(&self) -> MutexGuard<'_, AudioCore> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn downgrade(&self) -> std::sync::Weak<Mutex<AudioCore>> {
        Arc::downgrade(&self.0)
    }
}
