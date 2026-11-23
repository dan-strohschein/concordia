//! WAV (hound) and Ogg Vorbis (lewton) to interleaved `f32`; integers scale by 2^(bits−1).

use std::io::Cursor;
use std::num::NonZeroU32;
use std::sync::Arc;

use crate::error::AudioError;
use crate::sound::{Channels, SoundData};

/// Decodes a whole WAV file (8/16/24/32-bit integer or 32-bit float; mono or stereo).
pub fn decode_wav(bytes: &[u8]) -> Result<SoundData, AudioError> {
    let reader = hound::WavReader::new(Cursor::new(bytes)).map_err(wav)?;
    let spec = reader.spec();
    let channels = Channels::from_count(spec.channels)?;
    let rate = NonZeroU32::new(spec.sample_rate).ok_or_else(|| AudioError::Decode {
        format: "wav",
        reason: "sample rate 0".into(),
    })?;
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(wav)?,
        hound::SampleFormat::Int => {
            let scale = int_scale(spec.bits_per_sample)?;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|s| (f64::from(s) * scale) as f32))
                .collect::<Result<Vec<_>, _>>()
                .map_err(wav)?
        }
    };
    SoundData::new(rate, channels, samples)
}

/// 1 / 2^(bits−1) for 8–32-bit integers.
fn int_scale(bits: u16) -> Result<f64, AudioError> {
    if !(8..=32).contains(&bits) {
        return Err(AudioError::Decode {
            format: "wav",
            reason: format!("{bits}-bit samples are not supported"),
        });
    }
    Ok(1.0 / f64::from(1u32 << (bits - 1)))
}

fn wav(e: hound::Error) -> AudioError {
    AudioError::Decode {
        format: "wav",
        reason: e.to_string(),
    }
}

/// Decodes a whole Ogg Vorbis file.
pub fn decode_ogg(bytes: &[u8]) -> Result<SoundData, AudioError> {
    let mut stream = OggStream::open(Arc::from(bytes))?;
    let mut samples = Vec::new();
    while stream.read_packet(&mut samples)? {}
    let rate = NonZeroU32::new(stream.rate()).expect("checked when opened");
    SoundData::new(rate, stream.channels, samples)
}

/// An Ogg Vorbis reader that decodes packet by packet (for streaming).
pub struct OggStream {
    reader: lewton::inside_ogg::OggStreamReader<Cursor<Arc<[u8]>>>,
    channels: Channels,
    frames: u64,
}

impl std::fmt::Debug for OggStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OggStream")
            .field("rate", &self.reader.ident_hdr.audio_sample_rate)
            .field("channels", &self.channels)
            .finish()
    }
}

impl OggStream {
    /// Opens `bytes` and reads its headers.
    pub fn open(bytes: Arc<[u8]>) -> Result<Self, AudioError> {
        let reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes)).map_err(ogg)?;
        let channels = Channels::from_count(u16::from(reader.ident_hdr.audio_channels))?;
        if reader.ident_hdr.audio_sample_rate == 0 {
            return Err(AudioError::Decode {
                format: "ogg",
                reason: "sample rate 0".into(),
            });
        }
        Ok(Self {
            reader,
            channels,
            frames: 0,
        })
    }

    /// Frames per second.
    pub fn rate(&self) -> u32 {
        self.reader.ident_hdr.audio_sample_rate
    }

    /// Mono or stereo.
    pub fn channels(&self) -> Channels {
        self.channels
    }

    /// Appends the next packet's interleaved samples to `out`; false at the end of the stream.
    /// The final page's granule position trims the last block, so lengths are exact.
    pub fn read_packet(&mut self, out: &mut Vec<f32>) -> Result<bool, AudioError> {
        let Some(packet) = self.reader.read_dec_packet_itl().map_err(ogg)? else {
            return Ok(false);
        };
        let channels = self.channels.count();
        let mut frames = (packet.len() / channels) as u64;
        if let Some(end) = self.reader.get_last_absgp() {
            frames = frames.min(end.saturating_sub(self.frames));
        }
        self.frames += frames;
        let keep = frames as usize * channels;
        out.extend(packet[..keep].iter().map(|&s| f32::from(s) / 32768.0));
        Ok(true)
    }
}

fn ogg(e: lewton::VorbisError) -> AudioError {
    AudioError::Decode {
        format: "ogg",
        reason: e.to_string(),
    }
}
