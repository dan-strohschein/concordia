//! Where the renderer gets CPU asset data: the project's `AssetDb`, or in-memory assets.

use std::collections::BTreeMap;

use concordia_assets::{AssetRef, Builtin, BuiltinAsset, Material, MeshData, TextureData};
use concordia_core::id::AssetId;

pub use concordia_assets::AssetSource;

/// Assets held in memory (tests, procedural content), plus the built-ins.
#[derive(Debug, Clone, Default)]
pub struct MemoryAssets {
    meshes: BTreeMap<AssetRef, MeshData>,
    materials: BTreeMap<AssetRef, Material>,
    textures: BTreeMap<AssetRef, TextureData>,
}

/// The asset id under which in-memory assets live (sub-paths `<kind>/<name>`).
const MEMORY_ASSET: &str = "0193a1c2-7e40-7000-8000-00006d656d6f";

fn memory_ref(kind: &str, name: &str) -> AssetRef {
    let id: AssetId = MEMORY_ASSET
        .parse()
        .unwrap_or_else(|_| unreachable!("constant is a valid v7 id"));
    AssetRef::Asset {
        id,
        sub: Some(format!("{kind}/{name}")),
    }
}

impl MemoryAssets {
    /// Stores a mesh and returns its reference.
    pub fn insert_mesh(&mut self, name: &str, mesh: MeshData) -> AssetRef {
        let r = memory_ref("mesh", name);
        self.meshes.insert(r.clone(), mesh);
        r
    }

    /// Stores a material and returns its reference.
    pub fn insert_material(&mut self, name: &str, material: Material) -> AssetRef {
        let r = memory_ref("material", name);
        self.materials.insert(r.clone(), material);
        r
    }

    /// Stores a texture and returns its reference.
    pub fn insert_texture(&mut self, name: &str, texture: TextureData) -> AssetRef {
        let r = memory_ref("texture", name);
        self.textures.insert(r.clone(), texture);
        r
    }

    /// Whether any asset under `reference` is stored.
    pub fn contains(&self, reference: &AssetRef) -> bool {
        self.meshes.contains_key(reference)
            || self.materials.contains_key(reference)
            || self.textures.contains_key(reference)
    }

    /// Moves every asset of `other` in (replacing same references).
    pub fn merge(&mut self, other: MemoryAssets) {
        self.meshes.extend(other.meshes);
        self.materials.extend(other.materials);
        self.textures.extend(other.textures);
    }

    /// References of every stored asset.
    pub fn refs(&self) -> impl Iterator<Item = &AssetRef> {
        self.meshes
            .keys()
            .chain(self.materials.keys())
            .chain(self.textures.keys())
    }

    pub(crate) fn meshes(&self) -> impl Iterator<Item = (&AssetRef, &MeshData)> {
        self.meshes.iter()
    }

    pub(crate) fn materials(&self) -> impl Iterator<Item = (&AssetRef, &Material)> {
        self.materials.iter()
    }

    pub(crate) fn textures(&self) -> impl Iterator<Item = (&AssetRef, &TextureData)> {
        self.textures.iter()
    }

    /// Every stored asset with its kind.
    pub(crate) fn kinds(&self) -> impl Iterator<Item = (&AssetRef, AssetKind)> {
        let kind = |k| move |r| (r, k);
        self.meshes
            .keys()
            .map(kind(AssetKind::Mesh))
            .chain(self.materials.keys().map(kind(AssetKind::Material)))
            .chain(self.textures.keys().map(kind(AssetKind::Texture)))
    }
}

/// What an asset reference names, for reloading it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetKind {
    Mesh,
    Material,
    Texture,
}

/// The CPU asset data a frame of `world` needs that `held` does not report as already loaded:
/// meshes, materials (with their textures) and the environment map.
pub fn resolve(
    world: &crate::RenderWorld,
    source: &mut dyn AssetSource,
    held: &dyn Fn(&AssetRef) -> bool,
) -> Result<MemoryAssets, crate::RenderError> {
    let fail = |r: &AssetRef| {
        let reference = r.to_string();
        move |message: String| crate::RenderError::Asset { reference, message }
    };
    let mut out = MemoryAssets::default();
    let wanted = |out: &MemoryAssets, r: &AssetRef| !held(r) && !out.contains(r);
    for item in &world.draws {
        if wanted(&out, &item.mesh) {
            let data = source.mesh(&item.mesh).map_err(fail(&item.mesh))?;
            out.meshes.insert(item.mesh.clone(), data);
        }
        if wanted(&out, &item.material) {
            add_material(&mut out, source, &item.material, held)?;
        }
    }
    if let Some(env) = &world.environment
        && wanted(&out, &env.map)
    {
        out.textures.insert(
            env.map.clone(),
            source.texture(&env.map).map_err(fail(&env.map))?,
        );
    }
    Ok(out)
}

/// Loads `stale` assets again (they changed); materials bring textures not yet held.
pub(crate) fn reload(
    stale: &[(AssetRef, AssetKind)],
    source: &mut dyn AssetSource,
    held: &dyn Fn(&AssetRef) -> bool,
) -> Result<MemoryAssets, crate::RenderError> {
    let fail = |r: &AssetRef| {
        let reference = r.to_string();
        move |message: String| crate::RenderError::Asset { reference, message }
    };
    let mut out = MemoryAssets::default();
    for (reference, kind) in stale {
        match kind {
            AssetKind::Mesh => {
                let mesh = source.mesh(reference).map_err(fail(reference))?;
                out.meshes.insert(reference.clone(), mesh);
            }
            AssetKind::Texture => {
                let texture = source.texture(reference).map_err(fail(reference))?;
                out.textures.insert(reference.clone(), texture);
            }
            AssetKind::Material => add_material(&mut out, source, reference, held)?,
        }
    }
    Ok(out)
}

/// Adds a material and its textures that are neither held nor already in `out`.
fn add_material(
    out: &mut MemoryAssets,
    source: &mut dyn AssetSource,
    reference: &AssetRef,
    held: &dyn Fn(&AssetRef) -> bool,
) -> Result<(), crate::RenderError> {
    let fail = |r: &AssetRef| {
        let reference = r.to_string();
        move |message: String| crate::RenderError::Asset { reference, message }
    };
    let material = source.material(reference).map_err(fail(reference))?;
    for t in crate::material_rows::material_textures(&material) {
        if !held(t) && !out.contains(t) {
            out.textures
                .insert(t.clone(), source.texture(t).map_err(fail(t))?);
        }
    }
    out.materials.insert(reference.clone(), material);
    Ok(())
}

fn missing(reference: &AssetRef) -> String {
    format!("{reference} is not loaded")
}

impl AssetSource for MemoryAssets {
    fn mesh(&mut self, reference: &AssetRef) -> Result<MeshData, String> {
        match Builtin::of(reference) {
            Some(BuiltinAsset::Mesh(m)) => Ok(m),
            _ => self
                .meshes
                .get(reference)
                .cloned()
                .ok_or_else(|| missing(reference)),
        }
    }

    fn material(&mut self, reference: &AssetRef) -> Result<Material, String> {
        match Builtin::of(reference) {
            Some(BuiltinAsset::Material(m)) => Ok(m),
            _ => self
                .materials
                .get(reference)
                .cloned()
                .ok_or_else(|| missing(reference)),
        }
    }

    fn texture(&mut self, reference: &AssetRef) -> Result<TextureData, String> {
        match Builtin::of(reference) {
            Some(BuiltinAsset::Texture(t)) => Ok(t),
            _ => self
                .textures
                .get(reference)
                .cloned()
                .ok_or_else(|| missing(reference)),
        }
    }
}
