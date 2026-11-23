//! Streams: an Ogg reader on the engine side fills a preallocated ring of 2 s that a voice reads.
//! Loops are made here, by re-opening the reader, so the voice sees one continuous sequence:
//! the same frames, in the same order, as a decoded sound looping in memory.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::decode::OggStream;
use crate::error::AudioError;
use crate::mixer::MixerStats;
use crate::sound::Channels;

/// Seconds of audio a stream's ring holds.
const RING_SECONDS: usize = 2;

/// Shared between a feeder and its voice: the feeder has pushed its last frame.
#[derive(Debug, Default)]
pub(crate) struct StreamShared {
    finished: AtomicBool,
}

/// The voice's end of a stream.
pub(crate) struct StreamTap {
    samples: Consumer<f32>,
    shared: Arc<StreamShared>,
    pub(crate) channels: Channels,
}

impl StreamTap {
    /// The next frame; silence (counted) on an underrun, none once the feeder has finished.
    pub(crate) fn next_frame(&mut self, stats: &MixerStats) -> Option<[f32; 2]> {
        let n = self.channels.count();
        if self.samples.slots() < n {
            if self.shared.finished.load(Ordering::Acquire) && self.samples.slots() < n {
                return None;
            }
            stats.add_underrun();
            return Some([0.0; 2]);
        }
        let first = self.samples.pop().unwrap_or(0.0);
        let second = if n == 2 {
            self.samples.pop().unwrap_or(0.0)
        } else {
            first
        };
        Some([first, second])
    }
}

/// The engine's end of a stream: decodes into the ring.
struct Feeder {
    bytes: Arc<[u8]>,
    reader: OggStream,
    ring: Producer<f32>,
    shared: Arc<StreamShared>,
    packet: Vec<f32>,
    used: usize,
    /// The frame index of `packet[used]` in the source.
    frame: u64,
    looping: bool,
    loop_start: u64,
    loop_end: Option<u64>,
    done: bool,
}

/// What a new stream needs.
pub(crate) struct StreamSpec {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) looping: bool,
    pub(crate) loop_start: u64,
    pub(crate) loop_end: Option<u64>,
}

/// Opens a stream, fills its ring and returns the voice's tap and the source rate.
pub(crate) fn open(
    spec: StreamSpec,
    pump: &mut StreamPump,
) -> Result<(StreamTap, u32), AudioError> {
    let reader = OggStream::open(spec.bytes.clone())?;
    let channels = reader.channels();
    let rate = reader.rate();
    let capacity = rate as usize * channels.count() * RING_SECONDS;
    let (ring, samples) = RingBuffer::new(capacity);
    let shared = Arc::new(StreamShared::default());
    let mut feeder = Feeder {
        bytes: spec.bytes,
        reader,
        ring,
        shared: shared.clone(),
        packet: Vec::new(),
        used: 0,
        frame: 0,
        looping: spec.looping,
        loop_start: spec.loop_start,
        loop_end: spec.loop_end,
        done: false,
    };
    feeder.fill()?;
    pump.feeders.push(feeder);
    Ok((
        StreamTap {
            samples,
            shared,
            channels,
        },
        rate,
    ))
}

impl Feeder {
    /// Pushes whole frames until the ring is full or the stream ends.
    fn fill(&mut self) -> Result<(), AudioError> {
        let n = self.reader.channels().count();
        while !self.done && self.ring.slots() >= n {
            if self.used >= self.packet.len() && !self.refill()? {
                self.done = true;
                self.shared.finished.store(true, Ordering::Release);
                break;
            }
            if self.looping && self.loop_end.is_some_and(|end| self.frame >= end) {
                self.restart()?;
                continue;
            }
            for i in 0..n {
                // `slots() >= n` was checked, and only this feeder pushes.
                let _ = self.ring.push(self.packet[self.used + i]);
            }
            self.used += n;
            self.frame += 1;
        }
        Ok(())
    }

    /// Reads the next packet; at the end, loops (if looping) or reports the end.
    fn refill(&mut self) -> Result<bool, AudioError> {
        loop {
            self.packet.clear();
            self.used = 0;
            if self.reader.read_packet(&mut self.packet)? {
                if !self.packet.is_empty() {
                    return Ok(true);
                }
                continue;
            }
            if !self.looping {
                return Ok(false);
            }
            self.restart()?;
            if self.used < self.packet.len() {
                return Ok(true);
            }
        }
    }

    /// Re-opens the reader and skips to `loop_start`.
    fn restart(&mut self) -> Result<(), AudioError> {
        if !self.looping {
            self.done = true;
            self.shared.finished.store(true, Ordering::Release);
            return Ok(());
        }
        self.reader = OggStream::open(self.bytes.clone())?;
        let n = self.reader.channels().count();
        self.frame = 0;
        self.packet.clear();
        self.used = 0;
        while self.frame < self.loop_start {
            if self.used >= self.packet.len() {
                self.packet.clear();
                self.used = 0;
                if !self.reader.read_packet(&mut self.packet)? {
                    return Err(AudioError::Request(format!(
                        "loop_start {} is past the stream's end",
                        self.loop_start
                    )));
                }
                continue;
            }
            let skip =
                ((self.packet.len() - self.used) / n).min((self.loop_start - self.frame) as usize);
            self.used += skip * n;
            self.frame += skip as u64;
        }
        Ok(())
    }

    /// Whether its voice has gone (the tap was dropped).
    fn abandoned(&self) -> bool {
        self.ring.is_abandoned()
    }
}

/// Every live stream's feeder.
#[derive(Default)]
pub(crate) struct StreamPump {
    feeders: Vec<Feeder>,
}

impl StreamPump {
    /// Refills every ring and forgets streams whose voices have gone. A decode error ends
    /// that stream (it plays out what it has) and is returned for logging.
    pub(crate) fn pump(&mut self) -> Vec<AudioError> {
        self.feeders.retain(|f| !f.abandoned());
        let mut errors = Vec::new();
        for f in &mut self.feeders {
            if let Err(e) = f.fill() {
                f.done = true;
                f.shared.finished.store(true, Ordering::Release);
                errors.push(e);
            }
        }
        errors
    }

    /// Live streams.
    pub(crate) fn len(&self) -> usize {
        self.feeders.len()
    }
}
