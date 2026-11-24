//! Timer scopes on Metal: a counter sample buffer per timed submission, sampled at encoder (stage)
//! boundaries (Apple GPUs sample only there), resolved when the command buffer completes.

use std::time::Instant;

use concordia_rhi::{Command, CommandList, ScopeTime, TimerScope};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSRange;
use objc2_metal::{
    MTLBlitPassDescriptor, MTLComputePassDescriptor, MTLCounterSampleBuffer,
    MTLCounterSampleBufferDescriptor, MTLDevice, MTLRenderPassDescriptor, MTLStorageMode,
};

type SampleBuffer = ProtocolObject<dyn MTLCounterSampleBuffer>;
type MtlDevice = ProtocolObject<dyn MTLDevice>;

/// `MTLCounterErrorValue`: a sample the GPU didn't write.
const ERROR_VALUE: u64 = u64::MAX;
/// Largest sample buffer we allocate (Metal caps one at 32 KB of 8-byte samples).
const MAX_SAMPLES: usize = 4096;

/// A GPU timestamp paired with the CPU instant it was taken, to turn GPU ticks into time.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Calibration {
    gpu: u64,
    at: Instant,
}

impl Calibration {
    /// Samples `device`'s GPU clock now.
    pub(crate) fn sample(device: &MtlDevice) -> Self {
        let (mut cpu, mut gpu) = (0u64, 0u64);
        // SAFETY: both pointers are to live, writable u64 locals for the duration of the call.
        unsafe {
            device.sampleTimestamps_gpuTimestamp(
                std::ptr::NonNull::from(&mut cpu),
                std::ptr::NonNull::from(&mut gpu),
            );
        }
        Self {
            gpu,
            at: Instant::now(),
        }
    }

    /// Nanoseconds per GPU tick between `self` and a later sample (1 when the clock didn't move).
    fn ns_per_tick(&self, later: &Self) -> f64 {
        let ticks = later.gpu.saturating_sub(self.gpu);
        let ns = later.at.duration_since(self.at).as_nanos() as f64;
        if ticks == 0 || ns <= 0.0 {
            1.0
        } else {
            ns / ticks as f64
        }
    }
}

/// The encoders opened inside timer scopes, and where their samples go.
pub(crate) struct TimerPlan {
    buffer: Retained<SampleBuffer>,
    capacity: usize,
    /// (scope, first sample index, sample count) per timed encoder, in order.
    encoders: Vec<(TimerScope, usize, usize)>,
    open: Option<TimerScope>,
    scopes: Vec<TimerScope>,
}

impl TimerPlan {
    /// A plan for `list` if it has timer scopes and the device can sample them.
    pub(crate) fn for_list(device: &MtlDevice, list: &CommandList, enabled: bool) -> Option<Self> {
        if !enabled {
            return None;
        }
        let capacity = (4 * encoders_upper_bound(list)).min(MAX_SAMPLES);
        if capacity == 0 {
            return None;
        }
        let set = crate::probe::timestamp_counter_set(device)?;
        let desc = MTLCounterSampleBufferDescriptor::new();
        desc.setCounterSet(Some(&set));
        desc.setStorageMode(MTLStorageMode::Shared);
        // SAFETY: `capacity` is within Metal's 32 KB sample buffer limit (MAX_SAMPLES).
        unsafe { desc.setSampleCount(capacity) };
        let buffer = device
            .newCounterSampleBufferWithDescriptor_error(&desc)
            .ok()?;
        Some(Self {
            buffer,
            capacity,
            encoders: Vec::new(),
            open: None,
            scopes: Vec::new(),
        })
    }

    /// Opens `scope`: encoders until [`TimerPlan::end`] are sampled for it.
    pub(crate) fn begin(&mut self, scope: TimerScope) {
        self.open = Some(scope);
        self.scopes.push(scope);
    }

    /// Closes the open scope.
    pub(crate) fn end(&mut self) {
        self.open = None;
    }

    /// Reserves `count` samples for a new encoder in the open scope.
    fn reserve(&mut self, count: usize) -> Option<usize> {
        let scope = self.open?;
        let first = self.encoders.last().map_or(0, |(_, i, n)| i + n);
        if first + count > self.capacity {
            return None;
        }
        self.encoders.push((scope, first, count));
        Some(first)
    }

    /// Samples a render pass's vertex and fragment stages, if a scope is open.
    pub(crate) fn attach_render(&mut self, rp: &MTLRenderPassDescriptor) {
        let Some(first) = self.reserve(4) else { return };
        // SAFETY: index 0 always exists; the indices are inside the buffer (reserved above).
        unsafe {
            let a = rp.sampleBufferAttachments().objectAtIndexedSubscript(0);
            a.setSampleBuffer(Some(&self.buffer));
            a.setStartOfVertexSampleIndex(first);
            a.setEndOfVertexSampleIndex(first + 1);
            a.setStartOfFragmentSampleIndex(first + 2);
            a.setEndOfFragmentSampleIndex(first + 3);
        }
    }

    /// A compute pass descriptor sampling the encoder's start and end, if a scope is open.
    pub(crate) fn compute_pass(&mut self) -> Option<Retained<MTLComputePassDescriptor>> {
        let first = self.reserve(2)?;
        let desc = MTLComputePassDescriptor::computePassDescriptor();
        // SAFETY: index 0 always exists; the indices are inside the buffer (reserved above).
        unsafe {
            let a = desc.sampleBufferAttachments().objectAtIndexedSubscript(0);
            a.setSampleBuffer(Some(&self.buffer));
            a.setStartOfEncoderSampleIndex(first);
            a.setEndOfEncoderSampleIndex(first + 1);
        }
        Some(desc)
    }

    /// A blit pass descriptor sampling the encoder's start and end, if a scope is open.
    pub(crate) fn blit_pass(&mut self) -> Option<Retained<MTLBlitPassDescriptor>> {
        let first = self.reserve(2)?;
        let desc = MTLBlitPassDescriptor::blitPassDescriptor();
        // SAFETY: index 0 always exists; the indices are inside the buffer (reserved above).
        unsafe {
            let a = desc.sampleBufferAttachments().objectAtIndexedSubscript(0);
            a.setSampleBuffer(Some(&self.buffer));
            a.setStartOfEncoderSampleIndex(first);
            a.setEndOfEncoderSampleIndex(first + 1);
        }
        Some(desc)
    }

    /// Each scope's interval, once the command buffer has completed.
    pub(crate) fn resolve(&self, device: &MtlDevice, since: &Calibration) -> Vec<ScopeTime> {
        let used = self.encoders.last().map_or(0, |(_, i, n)| i + n);
        let ticks = self.samples(used);
        let ns_per_tick = since.ns_per_tick(&Calibration::sample(device));
        let origin = ticks.iter().copied().filter(|t| valid(*t)).min();
        let Some(origin) = origin else {
            return Vec::new();
        };
        let ms = |t: u64| (t - origin) as f64 * ns_per_tick / 1.0e6;
        self.scopes
            .iter()
            .filter_map(|&scope| {
                let mine = self
                    .encoders
                    .iter()
                    .filter(|(s, _, _)| *s == scope)
                    .flat_map(|(_, i, n)| ticks[*i..i + n].iter().copied())
                    .filter(|t| valid(*t));
                let (start, end) = mine.fold(None, |acc: Option<(u64, u64)>, t| {
                    Some(acc.map_or((t, t), |(a, b)| (a.min(t), b.max(t))))
                })?;
                Some(ScopeTime {
                    scope,
                    start_ms: ms(start),
                    end_ms: ms(end),
                })
            })
            .collect()
    }

    /// The first `count` resolved samples (error values where none was written).
    fn samples(&self, count: usize) -> Vec<u64> {
        if count == 0 {
            return Vec::new();
        }
        // SAFETY: the range lies inside the buffer, and the command buffer that wrote it is done.
        let data = unsafe {
            self.buffer.resolveCounterRange(NSRange {
                location: 0,
                length: count,
            })
        };
        let Some(data) = data else {
            return vec![ERROR_VALUE; count];
        };
        data.to_vec()
            .as_chunks::<8>()
            .0
            .iter()
            .map(|b| u64::from_ne_bytes(*b))
            .chain(std::iter::repeat(ERROR_VALUE))
            .take(count)
            .collect()
    }
}

/// A sample the GPU wrote (not the error value, and not zero, which Apple writes for stages that
/// didn't run).
fn valid(tick: u64) -> bool {
    tick != ERROR_VALUE && tick != 0
}

/// How many encoders `list` can open inside timer scopes, at most.
fn encoders_upper_bound(list: &CommandList) -> usize {
    let mut open = false;
    let mut count = 0;
    for command in &list.commands {
        match command {
            Command::BeginTimer(_) => open = true,
            Command::EndTimer => open = false,
            Command::BeginRenderPass(_)
            | Command::SetPipeline(_)
            | Command::Dispatch { .. }
            | Command::DispatchIndirect { .. }
            | Command::CopyBuffer { .. }
            | Command::CopyBufferToTexture { .. }
            | Command::CopyTextureToBuffer { .. }
                if open =>
            {
                count += 1
            }
            _ => {}
        }
    }
    count
}
