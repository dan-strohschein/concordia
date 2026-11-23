//! One voice: a source read frame by frame, resampled by linear interpolation, with gains,
//! looping and the stop ramp. Nothing here allocates.

use std::sync::Arc;

use crate::mixer::{MixerStats, STOP_RAMP_FRAMES, VoiceId};
use crate::sound::{Channels, SoundData};
use crate::stream::StreamTap;

/// Where a voice's frames come from.
pub(crate) enum Source {
    Memory {
        data: Arc<SoundData>,
        pos: usize,
        end: usize,
        loop_start: usize,
        looping: bool,
    },
    Stream(StreamTap),
}

impl Source {
    fn channels(&self) -> Channels {
        match self {
            Self::Memory { data, .. } => data.channels(),
            Self::Stream(tap) => tap.channels,
        }
    }

    /// The next frame as `[left, right]` (mono duplicated); none past the end.
    fn next_frame(&mut self, stats: &MixerStats) -> Option<[f32; 2]> {
        match self {
            Self::Memory {
                data,
                pos,
                end,
                loop_start,
                looping,
            } => {
                if *pos >= *end {
                    if !*looping {
                        return None;
                    }
                    *pos = *loop_start;
                }
                let frame = frame_at(data, *pos);
                *pos += 1;
                Some(frame)
            }
            Self::Stream(tap) => tap.next_frame(stats),
        }
    }
}

fn frame_at(data: &SoundData, frame: usize) -> [f32; 2] {
    let s = data.samples();
    match data.channels() {
        Channels::Mono => [s[frame]; 2],
        Channels::Stereo => [s[frame * 2], s[frame * 2 + 1]],
    }
}

/// A voice's settings and playhead.
pub(crate) struct Voice {
    pub(crate) id: VoiceId,
    pub(crate) source: Source,
    pub(crate) bus: usize,
    pub(crate) priority: u8,
    pub(crate) group: Option<(u64, u16)>,
    pub(crate) gain: f32,
    pub(crate) gains: [f32; 2],
    pub(crate) spatial: bool,
    pub(crate) step: f64,
    pub(crate) order: u64,
    pub(crate) stopping: Option<u32>,
    current: Option<[f32; 2]>,
    next: Option<[f32; 2]>,
    frac: f64,
    started: bool,
}

impl Voice {
    /// A voice that hasn't read its first frames yet.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: VoiceId,
        source: Source,
        bus: usize,
        priority: u8,
        group: Option<(u64, u16)>,
        gain: f32,
        gains: [f32; 2],
        spatial: bool,
        step: f64,
    ) -> Self {
        Self {
            id,
            source,
            bus,
            priority,
            group,
            gain,
            gains,
            spatial,
            step,
            order: 0,
            stopping: None,
            current: None,
            next: None,
            frac: 0.0,
            started: false,
        }
    }

    /// Starts the 5 ms ramp (once).
    pub(crate) fn stop(&mut self) {
        if self.stopping.is_none() {
            self.stopping = Some(STOP_RAMP_FRAMES);
        }
    }

    /// Whether it still counts toward its group's limit.
    pub(crate) fn counts(&self) -> bool {
        self.stopping.is_none()
    }

    /// Adds `frames` frames into `out` (interleaved stereo); false once it has ended.
    pub(crate) fn mix(&mut self, out: &mut [f32], stats: &MixerStats) -> bool {
        if !self.started {
            self.current = self.source.next_frame(stats);
            self.next = self.source.next_frame(stats);
            self.started = true;
        }
        let downmix = self.spatial && self.source.channels() == Channels::Stereo;
        for frame in out.as_chunks_mut::<2>().0 {
            let Some(current) = self.current else {
                return false;
            };
            let ramp = match self.stopping {
                Some(0) => return false,
                Some(n) => {
                    self.stopping = Some(n - 1);
                    n as f32 / STOP_RAMP_FRAMES as f32
                }
                None => 1.0,
            };
            let next = self.next.unwrap_or([0.0; 2]);
            let t = self.frac as f32;
            let mut s = [
                current[0] + (next[0] - current[0]) * t,
                current[1] + (next[1] - current[1]) * t,
            ];
            if downmix {
                let m = (s[0] + s[1]) * 0.5;
                s = [m, m];
            }
            let g = self.gain * ramp;
            frame[0] += s[0] * g * self.gains[0];
            frame[1] += s[1] * g * self.gains[1];
            self.advance(stats);
        }
        self.current.is_some() && self.stopping != Some(0)
    }

    fn advance(&mut self, stats: &MixerStats) {
        self.frac += self.step;
        while self.frac >= 1.0 {
            self.frac -= 1.0;
            self.current = self.next;
            self.next = if self.current.is_some() {
                self.source.next_frame(stats)
            } else {
                None
            };
        }
    }
}
