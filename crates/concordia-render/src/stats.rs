//! `render.stats`: the last frame the renderer drew (a capture or a viewport frame), recorded by
//! the render thread. Its GPU time arrives later from the device, possibly before the record.

use std::sync::{Arc, Mutex};

use concordia_api::{ApiError, Audience, CallCtx, Engine, Method};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::module::RenderState;
use concordia_assets::AssetRef;
use concordia_material::{ClosureReport, MergeKind};
use concordia_rhi::GpuTiming;

use crate::{RenderRequest, RenderStats, ViewMode};

/// What the renderer did for one frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct FrameStats {
    /// The renderer's frame number (counts captures and viewport frames).
    pub frame: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// How it was drawn.
    pub view_mode: ViewMode,
    /// CPU time to prepare, record and submit it (ms).
    pub cpu_ms: f64,
    /// GPU time of its commands (ms); null until measured, or when the device cannot tell.
    pub gpu_ms: Option<f64>,
    /// Whether its passes were timed: `off` unless perf mode is on, `unsupported` when the device
    /// cannot time passes.
    pub pass_timing: crate::perf::PassTiming,
    /// Each timed pass's GPU time, in order (empty unless `pass_timing` is `measured`, and until
    /// the GPU reports).
    pub passes: Vec<crate::perf::PassTime>,
    /// Meshes drawn.
    pub draws: u64,
    /// Meshes outside the camera's view (not drawn).
    pub culled: u64,
    /// Forward-pass draw calls (repeats are instanced).
    pub draw_calls: u64,
    /// Triangles drawn.
    pub triangles: u64,
    /// Point and spot lights in the world.
    pub punctual_lights: u64,
    /// The sun's shadow maps were reused from the previous frame.
    pub shadows_cached: bool,
    /// GPU memory the device has allocated, in bytes (null if the device cannot tell).
    pub memory_bytes: Option<u64>,
    /// Closures a pixel may evaluate (the platform's budget, or the project's lower one).
    pub closure_budget: u8,
    /// This frame's materials that were simplified to fit the closure budget.
    pub downgraded_materials: Vec<MaterialDowngrade>,
}

/// A material simplified to fit the closure budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MaterialDowngrade {
    /// The material asset.
    pub material: String,
    /// Closures before.
    pub before: u64,
    /// Closures after.
    pub after: u64,
    /// Merges in the order they were made.
    pub merges: Vec<ClosureMergeStats>,
}

/// One merge: the operator node made into a slab, the two slabs it combined and what was lost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ClosureMergeStats {
    /// The operator node.
    pub node: String,
    /// The operator collapsed: horizontal (a mix) or vertical (a layer).
    pub kind: MergedOperator,
    /// The merged slabs: `[a, b]` or `[top, bottom]`.
    pub slabs: Vec<String>,
    /// Inputs that could not be kept, as `node.field`.
    pub dropped: Vec<String>,
}

/// The operator a merge collapsed.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum MergedOperator {
    /// A horizontal mix.
    horizontal,
    /// A vertical layer.
    vertical,
}

/// The last frame's stats, its timed passes' names, and GPU timing that arrived before its frame
/// was recorded.
#[derive(Debug, Default)]
pub(crate) struct Last {
    frame: Option<FrameStats>,
    timed_passes: Vec<String>,
    early_gpu: Option<(u64, GpuTiming)>,
}

impl Last {
    /// Fills the recorded frame's GPU time and pass times from `timing`.
    fn apply(&mut self, timing: &GpuTiming) {
        if let Some(frame) = self.frame.as_mut() {
            frame.gpu_ms = timing.total_ms;
            frame.passes = crate::perf::pass_times(&self.timed_passes, timing);
        }
    }
}

/// Shared between the engine (reads) and the render thread (writes).
pub(crate) type LastFrame = Arc<Mutex<Last>>;

/// Records frame `number`, drawn for `request` in `cpu_ms`.
pub(crate) fn record(
    last: &LastFrame,
    number: u64,
    request: &RenderRequest,
    stats: RenderStats,
    cpu_ms: f64,
    renderer: &crate::Renderer,
) {
    let Ok(mut last) = last.lock() else { return };
    let early = match last.early_gpu.take() {
        Some((n, timing)) if n == number => Some(timing),
        _ => None,
    };
    last.timed_passes = stats.timed_passes;
    last.frame = Some(FrameStats {
        frame: number,
        width: request.width,
        height: request.height,
        view_mode: request.view_mode,
        cpu_ms,
        gpu_ms: None,
        pass_timing: stats.pass_timing,
        passes: Vec::new(),
        draws: stats.draws as u64,
        culled: stats.culled as u64,
        draw_calls: stats.draw_calls as u64,
        triangles: stats.triangles,
        punctual_lights: stats.punctual_lights as u64,
        shadows_cached: stats.shadows_cached,
        memory_bytes: stats.memory_bytes,
        closure_budget: renderer.closure_budget().get(),
        downgraded_materials: renderer
            .closure_reports()
            .iter()
            .map(|(material, report)| downgrade(material, report))
            .collect(),
    });
    if let Some(timing) = early {
        last.apply(&timing);
    }
}

fn downgrade(material: &AssetRef, report: &ClosureReport) -> MaterialDowngrade {
    MaterialDowngrade {
        material: material.to_string(),
        before: report.before as u64,
        after: report.after as u64,
        merges: report
            .merges
            .iter()
            .map(|m| ClosureMergeStats {
                node: m.node.clone(),
                kind: match m.kind {
                    MergeKind::Horizontal => MergedOperator::horizontal,
                    MergeKind::Vertical => MergedOperator::vertical,
                },
                slabs: m.slabs.to_vec(),
                dropped: m.dropped.clone(),
            })
            .collect(),
    }
}

/// Fills in frame `number`'s GPU time when the device reports it.
pub(crate) fn gpu_callback(last: LastFrame, number: u64) -> concordia_rhi::GpuTimeCallback {
    Box::new(move |timing| {
        let Ok(mut last) = last.lock() else { return };
        match last.frame.as_ref() {
            Some(f) if f.frame == number => last.apply(&timing),
            _ => last.early_gpu = Some((number, timing)),
        }
    })
}

/// Report the last frame the renderer drew (a capture or the editor viewport): its size, view
/// mode, CPU and GPU time, draws, culled meshes, triangles and GPU memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RenderStatsGet;

/// `render.stats` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RenderStatsParams {}

/// `render.stats` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RenderStatsResult {
    /// The last frame (null before the first capture or viewport frame).
    pub frame: Option<FrameStats>,
}

impl Method for RenderStatsGet {
    const NAME: &'static str = "render.stats";
    const MUTATES: bool = false;
    const AUDIENCE: Audience = Audience::agent;
    type Params = RenderStatsParams;
    type Result = RenderStatsResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        _: RenderStatsParams,
    ) -> Result<RenderStatsResult, ApiError> {
        let frame = engine
            .state::<RenderState>()
            .and_then(|s| s.last.lock().ok().and_then(|l| l.frame.clone()));
        Ok(RenderStatsResult { frame })
    }
}
