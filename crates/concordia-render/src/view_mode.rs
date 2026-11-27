//! Debug views of what the renderer does (`capture.screenshot` / `viewport.overlays` `view_mode`).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// How the scene is drawn: the lit image, or a debug view of one input or cost.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum ViewMode {
    /// The final lit image.
    #[default]
    lit,
    /// Base color and emission without lighting.
    unlit,
    /// Triangle edges only, hidden ones included.
    wireframe,
    /// Shading normals as colors (x, y, z → red, green, blue).
    normals,
    /// Base color only.
    albedo,
    /// Roughness in green, metallic in blue.
    roughness_metallic,
    /// How many surfaces each pixel shades (brighter: more).
    overdraw,
    /// The lit image tinted by shadow cascade (red, green, blue, yellow from near to far).
    shadow_cascades,
    /// Lights per cluster as a heat map (blue: none, green: 8, red: 16 or more).
    light_clusters,
    /// Closures per pixel as a heat map (blue: one, green: four, red: eight).
    closures,
    /// Haze weight (white: all haze).
    haze,
    /// Fuzz (sheen) color.
    fuzz,
    /// Subsurface wrap per channel (brighter: light reaches further past the terminator).
    subsurface,
    /// Thin-film thickness (white: 1000 nm or more).
    thin_film,
    /// Level of detail: hue by simulation LOD (white none; green to red for 0–3), instanced dimmed.
    lod,
    /// One color per meshlet (ADR-0100); surfaces off the meshlet path are grey.
    meshlets,
    /// One color per triangle.
    triangles,
    /// Motion vectors: black where still, hue by screen direction, brighter with length (full
    /// at 16 pixels).
    motion,
    /// Meshlets tinted by the culling pass that drew them: pass 1 green, pass 2 orange (two-pass
    /// occlusion, M4 4.4b); M1 draws grey.
    occlusion,
    /// The HZB's finest level as grey depth (brighter: nearer); black when the frame has no HZB.
    hzb,
    /// The lit image tinted by the virtual-shadow-map page each pixel reads (one colour per
    /// physical page, page borders dark); untinted without virtual shadow maps (M4 4.7).
    vsm_pages,
    /// The lit image tinted by the clipmap level each pixel reads (a hue per level, magenta where it
    /// fell back to a coarser level).
    vsm_levels,
    /// The lit image tinted by the page's cache state: yellow drawn this frame, green kept from an
    /// earlier frame, blue with a dynamic layer.
    vsm_cache,
    /// The sun's visibility alone: white lit, black shadowed (CSM or VSM; not tonemapped).
    shadow_mask,
}

impl ViewMode {
    /// The mode number `forward.slang` switches on.
    pub(crate) fn shader_index(self) -> u32 {
        self as u32
    }

    /// Whether the image is tonemapped (debug data is shown as is, clamped).
    pub(crate) fn tonemapped(self) -> bool {
        matches!(
            self,
            Self::lit
                | Self::shadow_cascades
                | Self::light_clusters
                | Self::vsm_pages
                | Self::vsm_levels
                | Self::vsm_cache
        )
    }

    /// Whether the environment's sky is drawn behind the scene.
    pub(crate) fn draws_sky(self) -> bool {
        matches!(
            self,
            Self::lit
                | Self::unlit
                | Self::shadow_cascades
                | Self::light_clusters
                | Self::vsm_pages
                | Self::vsm_levels
                | Self::vsm_cache
        )
    }
}
