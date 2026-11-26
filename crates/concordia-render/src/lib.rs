//! Concordia's renderer: a render graph with automatic barriers and transient aliasing, forward
//! PBR, cameras, headless capture and `capture.screenshot` (ADR-0040).

mod assets;
mod budget;
mod camera;
pub mod cluster;
mod components;
mod cull;
mod cull_stats;
mod environment;
mod error;
mod extract;
mod forward_pass;
mod frame;
mod frame_log;
mod gizmo;
mod gpu;
pub mod graph;
mod hzb;
pub mod ibl;
mod lighting;
mod material_rows;
mod meshlet;
mod meshlet_pass;
mod module;
mod motion;
mod orbit;
mod overlay;
pub mod perf;
mod pipelines;
mod pointer;
mod programs;
mod renderer;
mod representation;
mod scan;
mod scene_cache;
pub mod shadow;
mod shadow_pass;
mod shape;
mod skinning;
mod stats;
mod templates;
pub mod temporal;
#[cfg(test)]
mod test_gpu;
mod thread;
pub mod tonemap;
mod ui_pass;
mod view;
mod view_mode;
mod visibility;

pub use assets::{AssetSource, MemoryAssets, resolve};
pub use budget::{effective_closure_budget, platform_closure_budget};
pub use camera::view_matrix;
pub use components::{
    Camera, DirectionalLight, Environment, Msaa, PointLight, Projection, SpotLight, Tonemap,
};
pub use concordia_material::{ClosureBudget, ClosureReport};
pub use cull::{CullCode, CullView, Culling};
pub use cull_stats::CulledMeshlet;
pub use error::RenderError;
pub use extract::{
    CameraView, DrawItem, EntityItems, LightView, PoseView, PunctualKind, PunctualLight,
    RenderWorld, SkinView,
};
pub use frame::DrawPush;
pub use hzb::HzbPush;
pub use lighting::candela;
pub use meshlet::{CompactPush, CullPush, MeshletPush, TaskPush};
pub use module::{
    CaptureScreenshot, CaptureScreenshotParams, CaptureScreenshotResult, DeviceFactory,
    RenderModule, ViewportControl, ViewportNavigateParams, attach_surface, detach_surface,
    navigate, viewport_control,
};
pub use orbit::{OrbitCamera, Preset};
pub use overlay::{
    GizmoAxis, GizmoHandle, GizmoMode, GizmoOverlay, GizmoRing, Overlays, gizmo_handles, gizmo_hit,
    gizmo_pick, gizmo_rings, ring_hit,
};
pub use pipelines::{include_dirs, shader_dir};
pub use pointer::{
    ViewportPointerRay, ViewportPointerRayParams, ViewportPointerRayResult, camera_ray,
    clip_from_world, scene_camera, world_ui_lens,
};
pub use renderer::{
    FrameCallbacks, Image, MAX_SIZE, MotionImage, RenderRequest, RenderStats, Renderer,
};
pub use representation::Representation;
pub use scan::{ScanAddPush, ScanPush};
pub use scene_cache::{RenderScene, SceneCache, SceneDelta, SceneItems};
pub use shadow_pass::ShadowPush;
pub use stats::{
    ClosureMergeStats, FrameStats, MaterialDowngrade, MergedOperator, MeshletCounts, RasterPath,
    RenderStatsGet, RenderStatsParams, RenderStatsResult,
};
pub use ui_pass::{UI_QUAD_FLOATS, UI_STOP_FLOATS, UiPush, encode_ui};
pub use view::{Landing, Pick, View, axis_delta, landing, pick, rotate_angle, scale_factor};
pub use view_mode::ViewMode;

/// Registers the render components.
pub fn register_types(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_render_types::register_components(registry)?;
    concordia_terra::register_component::<Representation>(registry)
}
