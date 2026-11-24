//! Render components: defined in `concordia-render-types` (shared with the path tracer).

pub use concordia_render_types::{
    Camera, DirectionalLight, Environment, Msaa, PointLight, Projection, SpotLight, Tonemap,
};

/// The mode index `tonemap.slang` switches on.
pub(crate) fn tonemap_shader_mode(tonemap: Tonemap) -> u32 {
    match tonemap {
        Tonemap::AgX => 0,
        Tonemap::KhronosPbrNeutral => 1,
        Tonemap::AcesFitted => 2,
        Tonemap::Linear => 3,
    }
}
