//! Concordia's renderer: a render graph with automatic barriers and transient aliasing, forward
//! PBR, cameras, headless capture and `capture.screenshot` (ADR-0040).

mod assets;
mod camera;
pub mod cluster;
mod components;
mod cull;
mod environment;
mod error;
mod extract;
mod frame;
mod frame_log;
mod gizmo;
mod gpu;
pub mod graph;
pub mod ibl;
mod lighting;
mod module;
mod orbit;
mod pipelines;
mod renderer;
mod scene_cache;
pub mod shadow;
mod shadow_pass;
mod thread;
pub mod tonemap;
mod view;

pub use assets::{AssetSource, MemoryAssets, resolve};
pub use camera::view_matrix;
pub use components::{
    Camera, DirectionalLight, Environment, Msaa, PointLight, Projection, SpotLight, Tonemap,
};
pub use error::RenderError;
pub use extract::{
    CameraView, DrawItem, EntityItems, LightView, PunctualKind, PunctualLight, RenderWorld,
};
pub use frame::DrawPush;
pub use lighting::candela;
pub use module::{
    CaptureScreenshot, CaptureScreenshotParams, CaptureScreenshotResult, DeviceFactory,
    RenderModule, ViewportControl, ViewportNavigateParams, attach_surface, navigate,
    viewport_control,
};
pub use orbit::{OrbitCamera, Preset};
pub use pipelines::shader_dir;
pub use renderer::{FrameCallbacks, Image, MAX_SIZE, RenderRequest, RenderStats, Renderer};
pub use scene_cache::{RenderScene, SceneCache, SceneDelta, SceneItems};
pub use shadow_pass::ShadowPush;
pub use view::{Pick, View, axis_delta, pick};

/// Registers the render components.
pub fn register_types(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_terra::register_component::<Camera>(registry)?;
    concordia_terra::register_component::<DirectionalLight>(registry)?;
    concordia_terra::register_component::<PointLight>(registry)?;
    concordia_terra::register_component::<SpotLight>(registry)?;
    concordia_terra::register_component::<Environment>(registry)
}
