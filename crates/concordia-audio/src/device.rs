//! The `cpal` output: the default device, a mixer at the device's rate, and a callback that
//! only renders and converts (no allocation, lock or system call). The stream lives on its own
//! thread (cpal streams aren't `Send` everywhere) until the guard is dropped; a second thread
//! refills streams and frees retired voices every 50 ms.

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use crate::bus::BusGraph;
use crate::core::{AudioDevice, OpenDevice};
use crate::error::AudioError;
use crate::mixer::{Mixer, MixerConfig, MixerHandle};

/// Frames the callback renders per internal block (it loops for larger device buffers).
const SCRATCH_FRAMES: usize = 4096;

/// The default output device through `cpal`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpalDevice;

fn device_error(e: impl std::fmt::Display) -> AudioError {
    AudioError::Config(format!("audio device: {e}"))
}

impl AudioDevice for CpalDevice {
    fn open(&self, buses: &BusGraph, voices: NonZeroUsize) -> Result<OpenDevice, AudioError> {
        let (opened_tx, opened_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let buses = buses.clone();
        thread::Builder::new()
            .name("concordia-audio-device".into())
            .spawn(move || match start(&buses, voices) {
                Ok((stream, handle)) => {
                    let _ = opened_tx.send(Ok(handle));
                    // Holds the stream until the guard (the sender) is dropped.
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = opened_tx.send(Err(e));
                }
            })
            .map_err(device_error)?;
        let handle = opened_rx
            .recv()
            .map_err(|_| device_error("the device thread stopped"))??;
        Ok(OpenDevice {
            handle,
            guard: Box::new(stop_tx),
        })
    }
}

/// Opens the default device and starts its stream.
fn start(
    buses: &BusGraph,
    voices: NonZeroUsize,
) -> Result<(cpal::Stream, MixerHandle), AudioError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| device_error("no default output device"))?;
    let supported = device.default_output_config().map_err(device_error)?;
    let config: StreamConfig = supported.config();
    let rate = NonZeroU32::new(config.sample_rate).ok_or_else(|| device_error("rate 0"))?;
    let channels = usize::from(config.channels).max(1);
    let mut mixer_config = MixerConfig::new(buses.clone());
    mixer_config.rate = rate;
    mixer_config.voices = voices;
    let (mixer, handle) = Mixer::new(mixer_config)?;
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, channels, mixer),
        SampleFormat::I16 => build::<i16>(&device, config, channels, mixer),
        SampleFormat::U16 => build::<u16>(&device, config, channels, mixer),
        SampleFormat::I32 => build::<i32>(&device, config, channels, mixer),
        other => Err(device_error(format!("unsupported sample format {other}"))),
    }?;
    stream.play().map_err(device_error)?;
    Ok((stream, handle))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    channels: usize,
    mut mixer: Mixer,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * 2];
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| fill(out, channels, &mut mixer, &mut scratch),
            |e| tracing::warn!(target: "concordia::audio", error = %e, "audio device error"),
            None,
        )
        .map_err(device_error)
}

/// Renders into `out` (any channel count; stereo goes to the first two, mono gets the average).
fn fill<T: FromSample<f32>>(
    out: &mut [T],
    channels: usize,
    mixer: &mut Mixer,
    scratch: &mut [f32],
) {
    for chunk in out.chunks_mut(SCRATCH_FRAMES * channels) {
        let frames = chunk.len() / channels;
        let mix = &mut scratch[..frames * 2];
        mixer.render(mix);
        for (frame, lr) in chunk.chunks_mut(channels).zip(mix.chunks(2)) {
            for (c, sample) in frame.iter_mut().enumerate() {
                let v = match (channels, c) {
                    (1, _) => (lr[0] + lr[1]) * 0.5,
                    (_, 0) => lr[0],
                    (_, 1) => lr[1],
                    _ => 0.0,
                };
                *sample = T::from_sample_(v);
            }
        }
    }
}

/// Refills streams and frees retired voices on the engine side every 50 ms, while `core` lives.
pub(crate) fn spawn_pump(core: std::sync::Weak<std::sync::Mutex<crate::core::AudioCore>>) {
    let _ = thread::Builder::new()
        .name("concordia-audio-stream".into())
        .spawn(move || {
            while let Some(core) = core.upgrade() {
                {
                    let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(handle) = core.handle() {
                        for e in handle.pump_streams() {
                            tracing::warn!(target: "concordia::audio", error = %e, "a stream ended early");
                        }
                        handle.collect();
                    }
                }
                drop(core);
                thread::sleep(Duration::from_millis(50));
            }
        });
}
