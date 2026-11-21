//! The Substrate-style material model (ADR-0068): slabs combined by vertical layering, horizontal
//! mixing and coverage, stored as a flat map of named nodes (the contents of a `.material.ron`).

use std::collections::{BTreeMap, BTreeSet};

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::data::{AlphaMode, MaterialData};
use crate::reference::AssetRef;

/// A layered material: named nodes, the root node, and how the surface is drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct Material {
    /// Slabs and the operators combining them, by name.
    pub nodes: BTreeMap<String, MaterialNode>,
    /// The node whose result is the surface.
    pub root: String,
    /// Alpha handling.
    pub alpha_mode: AlphaMode,
    /// Render both faces.
    pub double_sided: bool,
    /// Ignore lighting: the root's base color plus emission.
    pub unlit: bool,
}

/// One node of a material: a slab or an operator over other nodes (by name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[allow(
    clippy::large_enum_variant,
    reason = "a material has a few nodes; reflection has no Box"
)]
pub enum MaterialNode {
    /// A slab: one BSDF closure.
    Slab(Slab),
    /// `top` layered over `bottom`; light reaches the bottom through the top.
    Vertical {
        /// The upper node.
        top: String,
        /// The lower node.
        bottom: String,
    },
    /// `a` and `b` side by side; `mix` of the surface is `b`.
    Horizontal {
        /// The node at mix 0.
        a: String,
        /// The node at mix 1.
        b: String,
        /// Fraction of the surface that is `b`.
        #[reflect(range(min = 0.0, max = 1.0))]
        mix: f32,
        /// Per-pixel mix (R, linear), multiplied by `mix`.
        mix_texture: Option<AssetRef>,
    },
    /// `node` covers only `coverage` of the surface; the rest shows what is below.
    Coverage {
        /// The covering node.
        node: String,
        /// Covered fraction.
        #[reflect(range(min = 0.0, max = 1.0))]
        coverage: f32,
    },
}

/// A slab: an interface (specular) over a medium (diffuse), with optional features.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct Slab {
    /// Linear base color; alpha is the slab's opacity.
    #[reflect(widget = "color")]
    pub base_color: [f32; 4],
    /// Base color texture (sRGB).
    pub base_color_texture: Option<AssetRef>,
    /// Metalness 0..1.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub metallic: f32,
    /// Dielectric specular 0..1 (F0 = 0.08 × specular; 0.5 is F0 0.04).
    #[reflect(range(min = 0.0, max = 1.0))]
    pub specular: f32,
    /// Perceptual roughness 0..1.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub roughness: f32,
    /// Metallic (B) and roughness (G) texture (linear).
    pub metallic_roughness_texture: Option<AssetRef>,
    /// Tangent-space normal map (linear).
    pub normal_texture: Option<AssetRef>,
    /// Normal map strength.
    #[reflect(range(min = 0.0, max = 10.0))]
    pub normal_scale: f32,
    /// Ambient occlusion texture (R, linear).
    pub occlusion_texture: Option<AssetRef>,
    /// Occlusion strength.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub occlusion_strength: f32,
    /// Linear emissive color.
    #[reflect(widget = "color")]
    pub emissive: [f32; 3],
    /// Emissive multiplier.
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub emissive_strength: f32,
    /// Emissive texture (sRGB).
    pub emissive_texture: Option<AssetRef>,
    /// Light passing through the slab's medium at normal incidence (matters above another node).
    #[reflect(widget = "color")]
    pub transmittance: [f32; 3],
    /// A second, blended specular lobe.
    pub haze: Option<Haze>,
    /// A sheen lobe for cloth and dust.
    pub fuzz: Option<Fuzz>,
    /// Light scattering under the surface (a forward approximation).
    pub subsurface: Option<Subsurface>,
    /// An interference film on the interface.
    pub thin_film: Option<ThinFilm>,
}

/// Haze: a second specular lobe sharing the slab's F0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Haze {
    /// Perceptual roughness of the second lobe.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub roughness: f32,
    /// Weight of the second lobe.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub weight: f32,
}

/// Fuzz: a Charlie sheen lobe over the slab.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Fuzz {
    /// Sheen color (its maximum is the fuzz amount).
    #[reflect(widget = "color")]
    pub color: [f32; 3],
    /// Perceptual sheen roughness.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub roughness: f32,
}

/// Subsurface scattering: wrapped diffuse per channel from the mean free path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Subsurface {
    /// Mean free path per channel, in metres (0..0.1).
    pub mean_free_path: [f32; 3],
}

/// A thin dielectric film on the interface (soap bubbles, oil, anodized metal).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ThinFilm {
    /// Film thickness in nanometres.
    #[reflect(range(min = 0.0, max = 2000.0))]
    pub thickness_nm: f32,
    /// Film index of refraction.
    #[reflect(range(min = 1.0, max = 3.0))]
    pub ior: f32,
}

/// A material is malformed (each error names the node or field).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MaterialError {
    /// The root names no node.
    #[error("material root {root:?} names no node; nodes: {known}")]
    MissingRoot {
        /// The root.
        root: String,
        /// Node names.
        known: String,
    },
    /// An operator names a node that doesn't exist.
    #[error("node {from:?} refers to {node:?}, which doesn't exist")]
    MissingNode {
        /// The referring node.
        from: String,
        /// The missing name.
        node: String,
    },
    /// Nodes refer to each other in a cycle.
    #[error("material nodes form a cycle: {}", path.join(" -> "))]
    Cycle {
        /// The cycle, starting and ending at the same node.
        path: Vec<String>,
    },
    /// A node is not reachable from the root.
    #[error("node {node:?} is not used by the root {root:?}")]
    Unreachable {
        /// The unused node.
        node: String,
        /// The root.
        root: String,
    },
    /// A node name is empty or uses characters other than letters, digits, `_` and `-`.
    #[error("node name {0:?} must be non-empty and use only letters, digits, '_' and '-'")]
    BadName(String),
    /// A value is out of its range or not finite.
    #[error("{field}: {value} is outside {min}..={max}")]
    OutOfRange {
        /// The field, as `node.field[index]`.
        field: String,
        /// The value.
        value: f32,
        /// Lowest allowed.
        min: f32,
        /// Highest allowed.
        max: f32,
    },
    /// The text is not a material.
    #[error("material text: {0}")]
    Parse(String),
}

impl Default for Material {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::from([(SURFACE.to_owned(), MaterialNode::Slab(Slab::default()))]),
            root: SURFACE.to_owned(),
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            unlit: false,
        }
    }
}

impl From<MaterialData> for Material {
    fn from(pbr: MaterialData) -> Self {
        Self::from_pbr(&pbr)
    }
}

impl Default for Slab {
    fn default() -> Self {
        Self {
            base_color: [0.8, 0.8, 0.8, 1.0],
            base_color_texture: None,
            metallic: 0.0,
            specular: 0.5,
            roughness: 0.5,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive: [0.0; 3],
            emissive_strength: 1.0,
            emissive_texture: None,
            transmittance: [0.0; 3],
            haze: None,
            fuzz: None,
            subsurface: None,
            thin_film: None,
        }
    }
}

/// The node name a migrated or default material uses for its one slab.
pub const SURFACE: &str = "surface";

impl Material {
    /// Checks names, references, cycles, reachability and value ranges.
    pub fn validate(&self) -> Result<(), MaterialError> {
        for name in self.nodes.keys() {
            check_name(name)?;
        }
        if !self.nodes.contains_key(&self.root) {
            let known = self.nodes.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(MaterialError::MissingRoot {
                root: self.root.clone(),
                known,
            });
        }
        for (name, node) in &self.nodes {
            for child in node.children() {
                if !self.nodes.contains_key(child) {
                    return Err(MaterialError::MissingNode {
                        from: name.clone(),
                        node: child.to_owned(),
                    });
                }
            }
        }
        self.check_acyclic()?;
        let reachable = self.reachable();
        if let Some(node) = self.nodes.keys().find(|n| !reachable.contains(n.as_str())) {
            return Err(MaterialError::Unreachable {
                node: node.clone(),
                root: self.root.clone(),
            });
        }
        self.nodes
            .iter()
            .try_for_each(|(name, node)| node.check_ranges(name))
    }

    /// The one-slab material equal to an M1 metallic-roughness material.
    pub fn from_pbr(pbr: &MaterialData) -> Self {
        let slab = Slab {
            base_color: pbr.base_color,
            base_color_texture: pbr.base_color_texture.clone(),
            metallic: pbr.metallic,
            roughness: pbr.roughness,
            metallic_roughness_texture: pbr.metallic_roughness_texture.clone(),
            normal_texture: pbr.normal_texture.clone(),
            normal_scale: pbr.normal_scale,
            occlusion_texture: pbr.occlusion_texture.clone(),
            occlusion_strength: pbr.occlusion_strength,
            emissive: pbr.emissive,
            emissive_strength: pbr.emissive_strength,
            emissive_texture: pbr.emissive_texture.clone(),
            ..Slab::default()
        };
        Self {
            nodes: BTreeMap::from([(SURFACE.to_owned(), MaterialNode::Slab(slab))]),
            root: SURFACE.to_owned(),
            alpha_mode: pbr.alpha_mode,
            double_sided: pbr.double_sided,
            unlit: pbr.unlit,
        }
    }

    /// Nodes reachable from the root, in name order (missing names are skipped).
    pub fn reachable(&self) -> BTreeSet<&str> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![self.root.as_str()];
        while let Some(name) = stack.pop() {
            let Some((key, node)) = self.nodes.get_key_value(name) else {
                continue;
            };
            if seen.insert(key.as_str()) {
                stack.extend(node.children());
            }
        }
        seen
    }

    /// The node named `name`.
    pub fn node(&self, name: &str) -> Option<&MaterialNode> {
        self.nodes.get(name)
    }

    fn check_acyclic(&self) -> Result<(), MaterialError> {
        let mut done = BTreeSet::new();
        for start in self.nodes.keys() {
            let mut path = Vec::new();
            self.visit(start, &mut path, &mut done)?;
        }
        Ok(())
    }

    /// Depth-first search; `path` holds the nodes being visited, `done` the finished ones.
    fn visit<'a>(
        &'a self,
        name: &'a str,
        path: &mut Vec<&'a str>,
        done: &mut BTreeSet<&'a str>,
    ) -> Result<(), MaterialError> {
        if done.contains(name) {
            return Ok(());
        }
        if let Some(at) = path.iter().position(|n| *n == name) {
            let mut cycle: Vec<String> = path[at..].iter().map(|n| (*n).to_owned()).collect();
            cycle.push(name.to_owned());
            return Err(MaterialError::Cycle { path: cycle });
        }
        path.push(name);
        for child in self
            .nodes
            .get(name)
            .map(MaterialNode::children)
            .into_iter()
            .flatten()
        {
            self.visit(child, path, done)?;
        }
        path.pop();
        done.insert(name);
        Ok(())
    }
}

impl MaterialNode {
    /// The nodes this one combines (empty for a slab).
    pub fn children(&self) -> Vec<&str> {
        match self {
            Self::Slab(_) => Vec::new(),
            Self::Vertical { top, bottom } => vec![top, bottom],
            Self::Horizontal { a, b, .. } => vec![a, b],
            Self::Coverage { node, .. } => vec![node],
        }
    }

    fn check_ranges(&self, name: &str) -> Result<(), MaterialError> {
        match self {
            Self::Slab(slab) => slab.check_ranges(name),
            Self::Vertical { .. } => Ok(()),
            Self::Horizontal { mix, .. } => in_range(&format!("{name}.mix"), *mix, 0.0, 1.0),
            Self::Coverage { coverage, .. } => {
                in_range(&format!("{name}.coverage"), *coverage, 0.0, 1.0)
            }
        }
    }
}

impl Slab {
    fn check_ranges(&self, name: &str) -> Result<(), MaterialError> {
        let field = |f: &str| format!("{name}.{f}");
        all_in_range(&field("base_color"), &self.base_color, 0.0, 1.0)?;
        in_range(&field("metallic"), self.metallic, 0.0, 1.0)?;
        in_range(&field("specular"), self.specular, 0.0, 1.0)?;
        in_range(&field("roughness"), self.roughness, 0.0, 1.0)?;
        in_range(&field("normal_scale"), self.normal_scale, 0.0, 10.0)?;
        in_range(
            &field("occlusion_strength"),
            self.occlusion_strength,
            0.0,
            1.0,
        )?;
        all_in_range(&field("emissive"), &self.emissive, 0.0, f32::MAX)?;
        in_range(
            &field("emissive_strength"),
            self.emissive_strength,
            0.0,
            1.0e6,
        )?;
        all_in_range(&field("transmittance"), &self.transmittance, 0.0, 1.0)?;
        if let Some(h) = &self.haze {
            in_range(&field("haze.roughness"), h.roughness, 0.0, 1.0)?;
            in_range(&field("haze.weight"), h.weight, 0.0, 1.0)?;
        }
        if let Some(f) = &self.fuzz {
            all_in_range(&field("fuzz.color"), &f.color, 0.0, 1.0)?;
            in_range(&field("fuzz.roughness"), f.roughness, 0.0, 1.0)?;
        }
        if let Some(s) = &self.subsurface {
            all_in_range(
                &field("subsurface.mean_free_path"),
                &s.mean_free_path,
                0.0,
                MAX_MEAN_FREE_PATH,
            )?;
        }
        if let Some(t) = &self.thin_film {
            in_range(
                &field("thin_film.thickness_nm"),
                t.thickness_nm,
                0.0,
                2000.0,
            )?;
            in_range(&field("thin_film.ior"), t.ior, 1.0, 3.0)?;
        }
        Ok(())
    }
}

/// The longest subsurface mean free path, in metres.
pub const MAX_MEAN_FREE_PATH: f32 = 0.1;

fn check_name(name: &str) -> Result<(), MaterialError> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(MaterialError::BadName(name.to_owned()))
    }
}

fn in_range(field: &str, value: f32, min: f32, max: f32) -> Result<(), MaterialError> {
    if (min..=max).contains(&value) {
        return Ok(());
    }
    Err(MaterialError::OutOfRange {
        field: field.to_owned(),
        value,
        min,
        max,
    })
}

fn all_in_range(field: &str, values: &[f32], min: f32, max: f32) -> Result<(), MaterialError> {
    values
        .iter()
        .enumerate()
        .try_for_each(|(i, v)| in_range(&format!("{field}[{i}]"), *v, min, max))
}

/// Reads `.material.ron` text: the layered format, or an M1 flat material migrated to one slab.
pub fn read_material(text: &str) -> Result<Material, MaterialError> {
    let material = match concordia_scene::read_document::<Material>(text) {
        Ok(material) => material,
        Err(layered) => match concordia_scene::read_document::<MaterialData>(text) {
            Ok(pbr) => Material::from_pbr(&pbr),
            Err(_) => return Err(MaterialError::Parse(layered.to_string())),
        },
    };
    material.validate()?;
    Ok(material)
}

/// Writes a material as canonical `.material.ron` text.
pub fn write_material(material: &Material) -> Result<String, MaterialError> {
    material.validate()?;
    concordia_scene::write_document(material).map_err(|e| MaterialError::Parse(e.to_string()))
}
