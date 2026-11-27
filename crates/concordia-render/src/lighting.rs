//! Punctual lights for the GPU: shading rows (world space, exposed radiance) and view-space
//! bounds for clustering.

use concordia_core::math::{Mat4, Vec3, Vec4};

use crate::cluster::LightBounds;
use crate::frame::push_rows;
use crate::{PunctualKind, RenderWorld};

/// Packed lights and their clustering bounds.
pub(crate) struct LightData {
    /// Four `float4` rows per light: position+range, direction, radiance, spot scale/offset.
    pub(crate) rows: Vec<u8>,
    /// View-space bounding spheres, in the same order.
    pub(crate) bounds: Vec<LightBounds>,
    /// Lights beyond the frame budget (not drawn).
    pub(crate) dropped: usize,
}

pub use concordia_render_types::candela;

/// Packs at most `max` punctual lights, in entity order.
pub(crate) fn punctual_lights(
    world: &RenderWorld,
    view: &Mat4,
    exposure: f32,
    max: usize,
) -> LightData {
    let mut data = LightData {
        rows: Vec::new(),
        bounds: Vec::new(),
        dropped: world.punctual.len().saturating_sub(max),
    };
    for light in world.punctual.iter().take(max) {
        let radiance = Vec3::from_array(light.color) * candela(light.intensity_lumens) * exposure;
        // Spot attenuation is saturate(cos * scale + offset)²; points use scale 0, offset 1.
        let (scale, offset) = match light.kind {
            PunctualKind::Point => (0.0, 1.0),
            PunctualKind::Spot { inner, outer } => {
                let cone = concordia_render_types::SpotCone::new(inner, outer);
                (cone.scale, cone.offset)
            }
        };
        push_rows(
            &mut data.rows,
            &[
                light.position.extend(light.range),
                light.direction.extend(0.0),
                radiance.extend(0.0),
                Vec4::new(scale, offset, 0.0, 0.0),
            ],
        );
        data.bounds.push(LightBounds {
            center: view.transform_point3(light.position),
            radius: light.range,
        });
    }
    data
}

/// Writes each shadowed light's slot + 1 (4.7c) into the third float of its fourth row (as bits;
/// 0: unshadowed), for the first `max` lights as `punctual_lights` packed them.
pub(crate) fn mark_shadow_slots(
    rows: &mut [u8],
    world: &RenderWorld,
    max: usize,
    slot_of: impl Fn(concordia_core::id::EntityId) -> Option<usize>,
) {
    for (i, light) in world.punctual.iter().take(max).enumerate() {
        let Some(slot) = slot_of(light.entity) else {
            continue;
        };
        let at = i * 64 + 3 * 16 + 8;
        if let Some(word) = rows.get_mut(at..at + 4) {
            word.copy_from_slice(&(slot as u32 + 1).to_le_bytes());
        }
    }
}
