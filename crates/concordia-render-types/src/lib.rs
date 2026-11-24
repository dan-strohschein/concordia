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
pub use shadow_settings::{CascadeSettings, PcfKernel};
pub use units::{SpotCone, candela, distance_window};
