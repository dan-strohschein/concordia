//! Where the renderer gets CPU asset data: the project's `AssetDb`, or in-memory assets.

use std::collections::BTreeMap;

use concordia_assets::{
    AssetDb, AssetRef, Builtin, BuiltinAsset, MaterialData, MeshData, TextureData,
};
use concordia_core::id::AssetId;

/// Loads mesh, material and texture data by reference (errors are human-readable).
pub trait AssetSource {
    /// A mesh.
    fn mesh(&mut self, reference: &AssetRef) -> Result<MeshData, String>;
    /// A material.
    fn material(&mut self, reference: &AssetRef) -> Result<MaterialData, String>;
    /// A texture.
    fn texture(&mut self, reference: &AssetRef) -> Result<TextureData, String>;
}

impl AssetSource for AssetDb {
    fn mesh(&mut self, reference: &AssetRef) -> Result<MeshData, String> {
        self.load_mesh(reference).map_err(|e| e.to_string())
    }

    fn material(&mut self, reference: &AssetRef) -> Result<MaterialData, String> {
        self.load_material(reference).map_err(|e| e.to_string())
    }

    fn texture(&mut self, reference: &AssetRef) -> Result<TextureData, String> {
        self.load_texture(reference).map_err(|e| e.to_string())
    }
}

/// Assets held in memory (tests, procedural content), plus the built-ins.
#[derive(Debug, Clone, Default)]
pub struct MemoryAssets {
    meshes: BTreeMap<AssetRef, MeshData>,
    materials: BTreeMap<AssetRef, MaterialData>,
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
    pub fn insert_material(&mut self, name: &str, material: MaterialData) -> AssetRef {
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

    fn material(&mut self, reference: &AssetRef) -> Result<MaterialData, String> {
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
