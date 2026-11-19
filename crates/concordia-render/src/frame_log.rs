//! Recent native-viewport frames for `viewport.stats`: queued on the engine thread, filled in by
//! the render thread and the display's presented callback.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// One native-viewport frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct FrameRecord {
    /// The caller's tag for the view this frame shows.
    pub seq: Option<u64>,
    /// When the input behind this view happened (Unix ms, from the caller).
    pub input_time_ms: Option<f64>,
    /// When the engine queued the frame for the render thread (Unix ms).
    pub queued_ms: f64,
    /// CPU time the render thread spent rendering and submitting the frame.
    pub cpu_ms: f64,
    /// When the frame was submitted to the GPU (Unix ms); null until it renders.
    pub submitted_ms: Option<f64>,
    /// When it reached the display (Unix ms); null if it has not (yet), or never did.
    pub presented_ms: Option<f64>,
    /// GPU time of the frame's commands (ms); null until measured.
    pub gpu_ms: Option<f64>,
}

/// Frames kept for `viewport.stats`.
pub(crate) const FRAME_RECORDS: usize = 4096;

/// Frame records by frame number, oldest first (shared with the render thread).
pub(crate) type FrameLog = Arc<Mutex<VecDeque<(u64, FrameRecord)>>>;

/// Unix time in milliseconds.
pub(crate) fn unix_ms(time: SystemTime) -> f64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// Appends frame `id`, dropping the oldest record when full.
pub(crate) fn push_frame(log: &FrameLog, id: u64, record: FrameRecord) {
    if let Ok(mut list) = log.lock() {
        if list.len() == FRAME_RECORDS {
            list.pop_front();
        }
        list.push_back((id, record));
    }
}

/// Updates frame `id`, if it is still logged.
pub(crate) fn update_frame(log: &FrameLog, id: u64, f: impl FnOnce(&mut FrameRecord)) {
    if let Ok(mut list) = log.lock()
        && let Some((_, record)) = list.iter_mut().rev().find(|(i, _)| *i == id)
    {
        f(record);
    }
}

/// Forgets frame `id` (skipped for a newer view, or never presented).
pub(crate) fn remove_frame(log: &FrameLog, id: u64) {
    if let Ok(mut list) = log.lock() {
        list.retain(|(i, _)| *i != id);
    }
}

/// Records frame `id`'s GPU time.
pub(crate) fn gpu_time_callback(log: FrameLog, id: u64) -> concordia_rhi::GpuTimeCallback {
    Box::new(move |ms| update_frame(&log, id, |f| f.gpu_ms = ms))
}

/// Records when frame `id` reached the display.
pub(crate) fn presented_callback(log: FrameLog, id: u64) -> concordia_rhi::PresentedCallback {
    Box::new(move |at| {
        if let Some(at) = at {
            update_frame(&log, id, |f| f.presented_ms = Some(unix_ms(at)));
        }
    })
}
