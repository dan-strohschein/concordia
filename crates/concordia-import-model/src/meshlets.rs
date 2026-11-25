//! The `meshlets` import setting: which static meshes get meshlets, built through the cache
//! (M4 4.2, ADR-0100).

use concordia_assets::{DerivedCache, MeshData};
use concordia_cluster::{ClusterInput, MeshletBuildSettings, MeshletMesh, build, cache_key};
use concordia_core::jobs::JobSystem;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

/// `auto` builds meshlets for static meshes with more than this many triangles (all parts).
pub const AUTO_MIN_TRIANGLES: usize = 8192;

/// The sidecar's `meshlets`: `"auto"`, `true` or `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeshletMode {
    /// Static meshes with more than [`AUTO_MIN_TRIANGLES`] triangles.
    #[default]
    Auto,
    /// Every static mesh; a skinned mesh is an error.
    Always,
    /// Never.
    Never,
}

impl<'de> Deserialize<'de> for MeshletMode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::Bool(true) => Ok(Self::Always),
            serde_json::Value::Bool(false) => Ok(Self::Never),
            serde_json::Value::String(s) if s == "auto" => Ok(Self::Auto),
            other => Err(D::Error::custom(format!(
                "meshlets must be \"auto\", true or false, not {other}"
            ))),
        }
    }
}

/// What a meshlet build needs from the import.
pub struct MeshletContext<'a> {
    /// The setting.
    pub mode: MeshletMode,
    /// The import job pool.
    pub jobs: &'a JobSystem,
    /// The build cache (the DDC in an import).
    pub cache: &'a dyn DerivedCache,
}

/// Fills `meshlets` on one source mesh's material parts, as `ctx.mode` says.
pub fn attach_meshlets(parts: &mut [MeshData], ctx: &MeshletContext<'_>) -> Result<(), String> {
    if !wanted(parts, ctx.mode)? {
        return Ok(());
    }
    let settings = MeshletBuildSettings::default();
    for part in parts {
        part.meshlets = meshlets_for(part, &settings, ctx)?;
    }
    Ok(())
}

/// Whether `mode` builds meshlets for a mesh made of `parts`.
fn wanted(parts: &[MeshData], mode: MeshletMode) -> Result<bool, String> {
    let skinned = parts
        .iter()
        .any(|p| !p.joints.is_empty() || !p.weights.is_empty());
    let triangles: usize = parts.iter().map(|p| p.indices.len() / 3).sum();
    match mode {
        MeshletMode::Never => Ok(false),
        MeshletMode::Auto => Ok(!skinned && triangles > AUTO_MIN_TRIANGLES),
        MeshletMode::Always if skinned => {
            Err("meshlets: true, but the mesh is skinned (meshlets are for static meshes)".into())
        }
        MeshletMode::Always => Ok(true),
    }
}

/// One part's encoded meshlets: from the cache when a valid entry exists, else built and cached.
fn meshlets_for(
    part: &MeshData,
    settings: &MeshletBuildSettings,
    ctx: &MeshletContext<'_>,
) -> Result<Vec<u8>, String> {
    let input =
        ClusterInput::new(&part.positions, &part.indices, None).map_err(|e| e.to_string())?;
    let key = cache_key(&input, settings);
    if let Some(bytes) = ctx.cache.get(&key).map_err(|e| e.to_string())?
        && MeshletMesh::decode(&bytes).is_ok()
    {
        return Ok(bytes);
    }
    let bytes = build(&input, settings, ctx.jobs)
        .and_then(|m| m.encode())
        .map_err(|e| e.to_string())?;
    ctx.cache.put(&key, &bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
