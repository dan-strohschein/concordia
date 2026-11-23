//! The mixer: a fixed voice pool summed into buses, children first, into interleaved stereo.
//! The engine side holds a `MixerHandle`; commands and retired sounds cross in `rtrb` rings, so
//! `Mixer::render` never allocates, frees, locks or blocks.

use std::num::{NonZeroU16, NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::bus::{BusGraph, BusIndex};
use crate::error::AudioError;
use crate::gain::db_to_gain;
use crate::sound::{LoadedBody, LoadedSound};
use crate::stream::{self, StreamPump, StreamSpec};
use crate::voice::{Source, Voice};

/// The mixer's default rate (frames per second).
pub const SAMPLE_RATE: u32 = 48_000;
/// The largest voice pool.
pub const MAX_VOICES: usize = 256;
/// Commands the queue holds before `QueueFull`.
pub const COMMAND_CAPACITY: usize = 1024;
/// A stop's linear ramp: 5 ms at 48 kHz.
pub const STOP_RAMP_FRAMES: u32 = 240;

/// How a mixer is set up.
#[derive(Debug, Clone, PartialEq)]
pub struct MixerConfig {
    /// Output frames per second.
    pub rate: NonZeroU32,
    /// The voice pool (1–256).
    pub voices: NonZeroUsize,
    /// The bus layout.
    pub buses: BusGraph,
    /// The most frames rendered in one internal block.
    pub max_block: NonZeroUsize,
}

impl MixerConfig {
    /// 48 kHz, 32 voices, 1,024-frame blocks.
    pub fn new(buses: BusGraph) -> Self {
        Self {
            rate: NonZeroU32::new(SAMPLE_RATE).expect("non-zero"),
            voices: NonZeroUsize::new(32).expect("non-zero"),
            buses,
            max_block: NonZeroUsize::new(1024).expect("non-zero"),
        }
    }
}

/// A playing voice, as the engine names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(pub u64);

/// What to play and how.
#[derive(Debug, Clone)]
pub struct PlayRequest {
    /// The sound.
    pub sound: LoadedSound,
    /// Linear volume (≥ 0).
    pub volume: f32,
    /// Playback-rate ratio (> 0, ≤ 16).
    pub pitch: f32,
    /// The bus.
    pub bus: BusIndex,
    /// Higher steals lower.
    pub priority: u8,
    /// Loops until stopped.
    pub looping: bool,
    /// Positioned: stereo sources are averaged to mono before `gains`.
    pub spatial: bool,
    /// `[left, right]` gains (pan and distance).
    pub gains: [f32; 2],
    /// Instance limit: (group key, most at once); the oldest of the group stops.
    pub group: Option<(u64, NonZeroU16)>,
}

impl PlayRequest {
    /// Unit volume and pitch, priority 128, unpositioned, no loop or limit.
    pub fn new(sound: LoadedSound, bus: BusIndex) -> Self {
        Self {
            sound,
            volume: 1.0,
            pitch: 1.0,
            bus,
            priority: 128,
            looping: false,
            spatial: false,
            gains: [1.0, 1.0],
            group: None,
        }
    }
}

/// Counters shared by the mixer and its handle.
#[derive(Debug)]
pub struct MixerStats {
    steals: AtomicU64,
    drops: AtomicU64,
    underruns: AtomicU64,
    active: AtomicU32,
    peaks: Vec<AtomicU32>,
}

impl MixerStats {
    fn new(buses: usize) -> Self {
        Self {
            steals: AtomicU64::new(0),
            drops: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            active: AtomicU32::new(0),
            peaks: (0..buses).map(|_| AtomicU32::new(0)).collect(),
        }
    }

    /// Voices stolen for new ones.
    pub fn steals(&self) -> u64 {
        self.steals.load(Ordering::Relaxed)
    }

    /// Requests dropped because every voice outranked them.
    pub fn drops(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }

    /// Stream samples that weren't ready in time.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Voices playing after the last render.
    pub fn active(&self) -> u32 {
        self.active.load(Ordering::Relaxed)
    }

    /// A bus's peak amplitude since the last `take_peak` of it.
    pub fn take_peak(&self, bus: BusIndex) -> f32 {
        self.peaks
            .get(bus.get())
            .map_or(0.0, |p| f32::from_bits(p.swap(0, Ordering::Relaxed)))
    }

    pub(crate) fn add_steal(&self) {
        self.steals.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn add_drop(&self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn add_underrun(&self) {
        self.underruns.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn set_active(&self, n: u32) {
        self.active.store(n, Ordering::Relaxed);
    }

    pub(crate) fn note_peak(&self, bus: usize, peak: f32) {
        if let Some(p) = self.peaks.get(bus) {
            // Non-negative floats order like their bits.
            p.fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
    }
}

fn check_gains(gains: [f32; 2]) -> Result<(), AudioError> {
    if gains.iter().all(|g| g.is_finite() && *g >= 0.0) {
        Ok(())
    } else {
        Err(AudioError::Request(format!(
            "gains {gains:?} must be finite and ≥ 0"
        )))
    }
}

fn check_request(r: &PlayRequest, buses: &BusGraph) -> Result<(), AudioError> {
    let bad = |m: String| Err(AudioError::Request(m));
    if !(r.volume.is_finite() && r.volume >= 0.0) {
        return bad(format!("volume {} must be finite and ≥ 0", r.volume));
    }
    if !(r.pitch.is_finite() && r.pitch > 0.0 && r.pitch <= 16.0) {
        return bad(format!("pitch {} must be in (0, 16]", r.pitch));
    }
    if r.bus.get() >= buses.len() {
        return bad(format!(
            "no bus {} in a layout of {}",
            r.bus.get(),
            buses.len()
        ));
    }
    check_gains(r.gains)?;
    let s = &r.sound;
    if !(s.gain.is_finite() && s.gain >= 0.0) {
        return bad(format!("sound gain {} must be finite and ≥ 0", s.gain));
    }
    if let LoadedBody::Memory(data) = &s.body {
        let frames = data.frames() as u64;
        if frames == 0 {
            return bad("the sound has no frames".into());
        }
        if r.looping {
            let end = s.loop_end.unwrap_or(frames);
            if end > frames || s.loop_start >= end {
                return bad(format!(
                    "loop {}..{end} must lie inside the sound's {frames} frames",
                    s.loop_start
                ));
            }
        }
    }
    Ok(())
}

/// Two distinct buffers, the first read and the second written.
fn two(bufs: &mut [Vec<f32>], a: usize, b: usize) -> (&[f32], &mut [f32]) {
    debug_assert_ne!(a, b);
    if a < b {
        let (lo, hi) = bufs.split_at_mut(b);
        (&lo[a], &mut hi[0])
    } else {
        let (lo, hi) = bufs.split_at_mut(a);
        (&hi[0], &mut lo[b])
    }
}

pub(crate) enum Command {
    Play { voice: Box<Voice> },
    Stop(VoiceId),
    Gains(VoiceId, [f32; 2]),
    Bus { bus: usize, gain: f32, mute: bool },
    StopAll,
}

pub(crate) enum Retired {
    Voice(Box<Voice>),
}

/// The audio-thread half: renders interleaved stereo.
pub struct Mixer {
    pub(crate) commands: Consumer<Command>,
    pub(crate) retired: Producer<Retired>,
    pub(crate) stats: Arc<MixerStats>,
    pub(crate) voices: Vec<Option<Box<Voice>>>,
    pub(crate) buses: BusGraph,
    pub(crate) bus_gain: Vec<f32>,
    pub(crate) bus_mute: Vec<bool>,
    pub(crate) bus_buf: Vec<Vec<f32>>,
    pub(crate) max_block: usize,
    pub(crate) order: u64,
}

impl std::fmt::Debug for Mixer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mixer")
            .field("voices", &self.voices.len())
            .field("buses", &self.buses.names())
            .finish()
    }
}

/// The engine half: queues commands and frees what the mixer retires.
pub struct MixerHandle {
    pub(crate) commands: Producer<Command>,
    pub(crate) retired: Consumer<Retired>,
    pub(crate) stats: Arc<MixerStats>,
    pub(crate) pump: StreamPump,
    pub(crate) buses: BusGraph,
    pub(crate) rate: NonZeroU32,
    pub(crate) next: u64,
}

impl std::fmt::Debug for MixerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MixerHandle")
            .field("rate", &self.rate)
            .field("next", &self.next)
            .finish()
    }
}

impl Mixer {
    /// A mixer and its handle.
    pub fn new(config: MixerConfig) -> Result<(Mixer, MixerHandle), AudioError> {
        if config.voices.get() > MAX_VOICES {
            return Err(AudioError::Config(format!(
                "{} voices; at most {MAX_VOICES}",
                config.voices
            )));
        }
        let (commands_in, commands_out) = RingBuffer::new(COMMAND_CAPACITY);
        let (retired_in, retired_out) = RingBuffer::new(COMMAND_CAPACITY + config.voices.get());
        let stats = Arc::new(MixerStats::new(config.buses.len()));
        let n = config.buses.len();
        let mixer = Mixer {
            commands: commands_out,
            retired: retired_in,
            stats: stats.clone(),
            voices: (0..config.voices.get()).map(|_| None).collect(),
            bus_gain: (0..n)
                .map(|b| crate::gain::db_to_gain(config.buses.gain_db(BusIndex(b))))
                .collect(),
            bus_mute: (0..n).map(|b| config.buses.muted(BusIndex(b))).collect(),
            bus_buf: (0..n)
                .map(|_| vec![0.0; config.max_block.get() * 2])
                .collect(),
            buses: config.buses.clone(),
            max_block: config.max_block.get(),
            order: 0,
        };
        let handle = MixerHandle {
            commands: commands_in,
            retired: retired_out,
            stats,
            pump: StreamPump::default(),
            buses: config.buses,
            rate: config.rate,
            next: 1,
        };
        Ok((mixer, handle))
    }

    /// Fills `out` (interleaved stereo) after applying queued commands. Never allocates, frees,
    /// locks or blocks.
    pub fn render(&mut self, out: &mut [f32]) {
        debug_assert!(out.len().is_multiple_of(2), "interleaved stereo");
        self.apply_commands();
        for block in out.chunks_mut(self.max_block * 2) {
            self.render_block(block);
        }
        let active = self.voices.iter().filter(|v| v.is_some()).count();
        self.stats.set_active(active as u32);
    }

    fn apply_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::Play { voice } => self.start(voice),
                Command::Stop(id) => self.with_voice(id, Voice::stop),
                Command::Gains(id, gains) => self.with_voice(id, |v| v.gains = gains),
                Command::Bus { bus, gain, mute } => {
                    if let (Some(g), Some(m)) =
                        (self.bus_gain.get_mut(bus), self.bus_mute.get_mut(bus))
                    {
                        *g = gain;
                        *m = mute;
                    }
                }
                Command::StopAll => self.voices.iter_mut().flatten().for_each(|v| v.stop()),
            }
        }
    }

    fn with_voice(&mut self, id: VoiceId, f: impl FnOnce(&mut Voice)) {
        if let Some(v) = self.voices.iter_mut().flatten().find(|v| v.id == id) {
            f(v);
        }
    }

    /// Puts a voice in a free slot, or steals the lowest-priority oldest one it outranks or ties.
    fn start(&mut self, mut voice: Box<Voice>) {
        self.order += 1;
        voice.order = self.order;
        if let Some((group, max)) = voice.group {
            self.limit_group(group, max);
        }
        let slot = match self.voices.iter().position(Option::is_none) {
            Some(free) => free,
            None => {
                let victim = self
                    .voices
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.as_ref().map(|v| (i, v.priority, v.order)))
                    .min_by_key(|&(_, priority, order)| (priority, order));
                match victim {
                    Some((i, priority, _)) if priority <= voice.priority => {
                        self.stats.add_steal();
                        self.retire(i);
                        i
                    }
                    _ => {
                        self.stats.add_drop();
                        self.send_back(voice);
                        return;
                    }
                }
            }
        };
        self.voices[slot] = Some(voice);
    }

    /// Ramps out the oldest counting voices of `group` until a new one fits under `max`.
    fn limit_group(&mut self, group: u64, max: u16) {
        loop {
            let mut count = 0;
            let mut oldest: Option<(usize, u64)> = None;
            for (i, v) in self.voices.iter().enumerate() {
                if let Some(v) = v
                    .as_ref()
                    .filter(|v| v.counts() && v.group.map(|g| g.0) == Some(group))
                {
                    count += 1;
                    if oldest.is_none_or(|(_, o)| v.order < o) {
                        oldest = Some((i, v.order));
                    }
                }
            }
            match oldest {
                Some((i, _)) if count >= usize::from(max) => {
                    if let Some(v) = self.voices[i].as_mut() {
                        v.stop();
                    }
                }
                _ => return,
            }
        }
    }

    fn render_block(&mut self, out: &mut [f32]) {
        let n = out.len();
        for buf in &mut self.bus_buf {
            buf[..n].fill(0.0);
        }
        for i in 0..self.voices.len() {
            let Some(voice) = self.voices[i].as_mut() else {
                continue;
            };
            let bus = voice.bus;
            if !voice.mix(&mut self.bus_buf[bus][..n], &self.stats) {
                self.retire(i);
            }
        }
        for k in 0..self.buses.order.len() {
            let bus = self.buses.order[k];
            let gain = if self.bus_mute[bus] {
                0.0
            } else {
                self.bus_gain[bus]
            };
            let mut peak = 0.0f32;
            for s in &mut self.bus_buf[bus][..n] {
                *s *= gain;
                peak = peak.max(s.abs());
            }
            self.stats.note_peak(bus, peak);
            if let Some(parent) = self.buses.parents[bus] {
                let (child, into) = two(&mut self.bus_buf, bus, parent);
                for (d, s) in into[..n].iter_mut().zip(&child[..n]) {
                    *d += *s;
                }
            }
        }
        out.copy_from_slice(&self.bus_buf[0][..n]);
    }

    /// Hands a voice back to the engine to be freed there.
    fn retire(&mut self, slot: usize) {
        if let Some(voice) = self.voices[slot].take() {
            self.send_back(voice);
        }
    }

    fn send_back(&mut self, voice: Box<Voice>) {
        if let Err(rtrb::PushError::Full(Retired::Voice(voice))) =
            self.retired.push(Retired::Voice(voice))
        {
            // The ring holds every voice and every queued command, so this can't happen; if it
            // ever did, leaking beats freeing on the audio thread.
            std::mem::forget(voice);
        }
    }
}

impl MixerHandle {
    /// Validates a request, opens its stream (if any) and queues the voice.
    pub fn play(&mut self, request: PlayRequest) -> Result<VoiceId, AudioError> {
        check_request(&request, &self.buses)?;
        if self.commands.is_full() {
            return Err(AudioError::QueueFull);
        }
        let id = VoiceId(self.next);
        let sound = &request.sound;
        let (source, source_rate) = match &sound.body {
            LoadedBody::Memory(data) => {
                let frames = data.frames();
                let end = if request.looping {
                    sound.loop_end.map_or(frames, |e| e as usize)
                } else {
                    frames
                };
                let source = Source::Memory {
                    data: data.clone(),
                    pos: 0,
                    end,
                    loop_start: sound.loop_start as usize,
                    looping: request.looping,
                };
                (source, data.rate().get())
            }
            LoadedBody::Stream(bytes) => {
                let spec = StreamSpec {
                    bytes: bytes.clone(),
                    looping: request.looping,
                    loop_start: sound.loop_start,
                    loop_end: sound.loop_end,
                };
                let (tap, rate) = stream::open(spec, &mut self.pump)?;
                (Source::Stream(tap), rate)
            }
        };
        let step = f64::from(source_rate) / f64::from(self.rate.get()) * f64::from(request.pitch);
        let voice = Voice::new(
            id,
            source,
            request.bus.get(),
            request.priority,
            request.group.map(|(g, max)| (g, max.get())),
            request.volume * sound.gain,
            request.gains,
            request.spatial,
            step,
        );
        self.push(Command::Play {
            voice: Box::new(voice),
        })?;
        self.next += 1;
        Ok(id)
    }

    /// Stops a voice with the 5 ms ramp (an ended voice is ignored).
    pub fn stop(&mut self, voice: VoiceId) -> Result<(), AudioError> {
        self.push(Command::Stop(voice))
    }

    /// Stops every voice with the ramp.
    pub fn stop_all(&mut self) -> Result<(), AudioError> {
        self.push(Command::StopAll)
    }

    /// Changes a voice's `[left, right]` gains.
    pub fn set_gains(&mut self, voice: VoiceId, gains: [f32; 2]) -> Result<(), AudioError> {
        check_gains(gains)?;
        self.push(Command::Gains(voice, gains))
    }

    /// Changes a bus's runtime gain and mute.
    pub fn set_bus(&mut self, bus: BusIndex, gain_db: f32, mute: bool) -> Result<(), AudioError> {
        if bus.get() >= self.buses.len() {
            return Err(AudioError::Request(format!("no bus {}", bus.get())));
        }
        if gain_db.is_nan() || gain_db > 24.0 {
            return Err(AudioError::Request(format!(
                "bus gain_db {gain_db} must be ≤ 24"
            )));
        }
        self.push(Command::Bus {
            bus: bus.get(),
            gain: db_to_gain(gain_db),
            mute,
        })
    }

    /// Refills every stream's ring; decode errors end their streams and are returned.
    pub fn pump_streams(&mut self) -> Vec<AudioError> {
        self.pump.pump()
    }

    /// Drops what the mixer has retired (here, on the engine thread).
    pub fn collect(&mut self) {
        while self.retired.pop().is_ok() {}
    }

    /// Live streams (for inspection).
    pub fn streams(&self) -> usize {
        self.pump.len()
    }

    fn push(&mut self, command: Command) -> Result<(), AudioError> {
        self.commands
            .push(command)
            .map_err(|_| AudioError::QueueFull)
    }

    /// The shared counters.
    pub fn stats(&self) -> &MixerStats {
        &self.stats
    }

    /// The bus layout.
    pub fn buses(&self) -> &BusGraph {
        &self.buses
    }

    /// Output frames per second.
    pub fn rate(&self) -> NonZeroU32 {
        self.rate
    }
}
