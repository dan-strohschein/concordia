//! Concordia's renderer: a render graph with automatic barriers and transient aliasing, forward
//! PBR, cameras, headless capture and `capture.screenshot` (ADR-0040).

mod assets;
mod camera;
pub mod cluster;
mod components;
mod error;
mod extract;
mod frame;
mod gpu;
pub mod graph;
mod lighting;
mod module;
mod pipelines;
mod renderer;
pub mod shadow;
mod shadow_pass;
pub mod tonemap;

pub use assets::{AssetSource, MemoryAssets};
pub use camera::view_matrix;
pub use components::{Camera, DirectionalLight, Msaa, PointLight, Projection, SpotLight, Tonemap};
pub use error::RenderError;
pub use extract::{CameraView, DrawItem, LightView, PunctualKind, PunctualLight, RenderWorld};
pub use frame::DrawPush;
pub use lighting::candela;
pub use module::{
    CaptureScreenshot, CaptureScreenshotParams, CaptureScreenshotResult, DeviceFactory,
    RenderModule,
};
pub use pipelines::shader_dir;
pub use renderer::{Image, MAX_SIZE, RenderRequest, RenderStats, Renderer};
pub use shadow_pass::ShadowPush;

/// Registers the render components.
pub fn register_types(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_terra::register_component::<Camera>(registry)?;
    concordia_terra::register_component::<DirectionalLight>(registry)?;
    concordia_terra::register_component::<PointLight>(registry)?;
    concordia_terra::register_component::<SpotLight>(registry)
}
