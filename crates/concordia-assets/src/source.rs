//! Where renderers get CPU asset data by reference: the project's `AssetDb`, or any other source
//! (the renderer's in-memory assets, test fixtures). Shared by the renderer and the path tracer.

use crate::{AssetDb, AssetRef, Material, MeshData, TextureData};

/// Loads mesh, material and texture data by reference (errors are human-readable).
pub trait AssetSource {
    /// A mesh.
    fn mesh(&mut self, reference: &AssetRef) -> Result<MeshData, String>;
    /// A material.
    fn material(&mut self, reference: &AssetRef) -> Result<Material, String>;
    /// A texture.
    fn texture(&mut self, reference: &AssetRef) -> Result<TextureData, String>;
}

impl AssetSource for AssetDb {
    fn mesh(&mut self, reference: &AssetRef) -> Result<MeshData, String> {
        self.load_mesh(reference).map_err(|e| e.to_string())
    }

    fn material(&mut self, reference: &AssetRef) -> Result<Material, String> {
        self.load_material(reference).map_err(|e| e.to_string())
    }

    fn texture(&mut self, reference: &AssetRef) -> Result<TextureData, String> {
        self.load_texture(reference).map_err(|e| e.to_string())
    }
}
