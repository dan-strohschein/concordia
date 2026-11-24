//! Choosing where to trace: the GPU by default, the CPU as the fallback (ADR-0101).

use std::sync::Arc;

use concordia_rhi::Device;
use concordia_shaders::ShaderCompiler;

use crate::cpu::{HdrImage, Settings, render_cpu};
use crate::gpu::GpuTracer;
use crate::{PathTraceError, Scene};

/// Where a render runs.
#[derive(Debug, Clone)]
pub enum Backend {
    /// The CPU (always available).
    Cpu,
    /// A GPU.
    Gpu(Arc<GpuTracer>),
}

impl Backend {
    /// The GPU on `device` when there is one and the tracer builds there, else the CPU.
    pub fn auto(device: Option<(Arc<dyn Device>, &dyn ShaderCompiler)>) -> Self {
        device
            .and_then(|(device, compiler)| GpuTracer::new(device, compiler).ok())
            .map_or(Self::Cpu, |gpu| Self::Gpu(Arc::new(gpu)))
    }
}

/// Renders `scene` on `backend`.
pub fn render(
    scene: &Scene,
    settings: &Settings,
    backend: &Backend,
) -> Result<HdrImage, PathTraceError> {
    match backend {
        Backend::Cpu => render_cpu(scene, settings),
        Backend::Gpu(gpu) => gpu.render(scene, settings),
    }
}
