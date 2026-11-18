//! Concordia's renderer: a render graph with automatic barriers and transient aliasing, forward
//! PBR, cameras, headless capture and `capture.screenshot` (ADR-0040).

mod assets;
mod camera;
mod components;
mod error;
mod extract;
mod frame;
mod gpu;
pub mod graph;
mod module;
mod pipelines;
mod renderer;

pub use assets::{AssetSource, MemoryAssets};
pub use camera::view_matrix;
pub use components::{Camera, DirectionalLight, Msaa, Projection, Tonemap};
pub use error::RenderError;
pub use extract::{CameraView, DrawItem, LightView, RenderWorld};
pub use frame::DrawPush;
pub use module::{
    CaptureScreenshot, CaptureScreenshotParams, CaptureScreenshotResult, DeviceFactory,
    RenderModule,
};
pub use pipelines::shader_dir;
pub use renderer::{Image, MAX_SIZE, RenderRequest, Renderer};

/// Registers the render components.
pub fn register_types(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_terra::register_component::<Camera>(registry)?;
    concordia_terra::register_component::<DirectionalLight>(registry)
}
