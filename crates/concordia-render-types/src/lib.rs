//! The render vocabulary shared by the GPU renderer (`concordia-render`) and the reference path
//! tracer (`concordia-pathtrace`): reflected render components, photometric units and the CPU
//! tone-mapping references. No GPU types or dependencies (ADR-0101).

mod camera;
mod components;
pub mod equirect;
mod shadow_settings;
pub mod tonemap;
mod units;

pub use camera::{ProjectionError, pick_camera, view_matrix};
pub use components::{
    Camera, DirectionalLight, Environment, Msaa, PointLight, Projection, SpotLight, Tonemap,
};
pub use shadow_settings::{CascadeSettings, LocalShadowSettings, PcfKernel};
pub use units::{SpotCone, candela, distance_window};

/// Registers the render components (`RenderModule` does this in a project; hosts without the
/// renderer, like the path tracer's tests, call it directly).
pub fn register_components(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_terra::register_component::<Camera>(registry)?;
    concordia_terra::register_component::<DirectionalLight>(registry)?;
    concordia_terra::register_component::<PointLight>(registry)?;
    concordia_terra::register_component::<SpotLight>(registry)?;
    concordia_terra::register_component::<Environment>(registry)
}
