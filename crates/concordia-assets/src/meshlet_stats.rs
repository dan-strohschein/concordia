//! Meshlet statistics of an asset's meshes, and why a mesh has none (`asset.meshlet_stats`,
//! `concordia cluster stats`; M4 4.2 Q7).

use std::collections::BTreeMap;

use concordia_api::{ApiError, CallCtx, Engine, Method};
use concordia_cluster::{AUTO_MIN_TRIANGLES, MeshletMesh, MeshletStats};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::module::{api_error, db};
use crate::{AssetDb, AssetError, AssetRef, MeshData};

/// One mesh sub-asset's meshlets, or why it has none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MeshMeshlets {
    /// The sub-asset path, e.g. "mesh/Rock/0".
    pub sub_asset: String,
    /// Triangles in the mesh.
    pub triangles: u32,
    /// The meshlet statistics, if the mesh has meshlets.
    pub meshlets: Option<MeshletStats>,
    /// Why the mesh has no meshlets (none when it has them).
    pub missing: Option<String>,
}

impl AssetDb {
    /// Meshlet statistics for every mesh of `asset` (or the one sub-asset it names).
    pub fn meshlet_stats(&mut self, asset: &AssetRef) -> Result<Vec<MeshMeshlets>, AssetError> {
        let not_found = || AssetError::NotFound(asset.to_string());
        let AssetRef::Asset { id, sub } = asset else {
            return Err(not_found());
        };
        let (subs, settings) = self.subs_and_settings(*id).ok_or_else(not_found)?;
        if sub
            .as_ref()
            .is_some_and(|s| !s.starts_with("mesh/") || !subs.contains(s))
        {
            return Err(not_found());
        }
        let mut meshes = Vec::new();
        for path in subs.into_iter().filter(|s| s.starts_with("mesh/")) {
            let reference = AssetRef::Asset {
                id: *id,
                sub: Some(path.clone()),
            };
            meshes.push((path, self.load_mesh(&reference)?));
        }
        let totals = mesh_totals(&meshes);
        meshes
            .iter()
            .filter(|(path, _)| sub.as_ref().is_none_or(|s| s == path))
            .map(|(path, mesh)| describe(path, mesh, totals[mesh_of(path)], &settings))
            .collect()
    }
}

/// The mesh a part belongs to: "mesh/<name>/<part>" → "mesh/<name>".
fn mesh_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(mesh, _)| mesh)
}

/// Triangles per mesh, over all its parts (the `auto` rule counts whole meshes).
fn mesh_totals(meshes: &[(String, MeshData)]) -> BTreeMap<&str, usize> {
    let mut totals = BTreeMap::new();
    for (path, mesh) in meshes {
        *totals.entry(mesh_of(path)).or_default() += mesh.indices.len() / 3;
    }
    totals
}

/// One part's entry: its statistics, or why it has no meshlets.
fn describe(
    path: &str,
    mesh: &MeshData,
    mesh_triangles: usize,
    settings: &serde_json::Value,
) -> Result<MeshMeshlets, AssetError> {
    let meshlets = match mesh.meshlets.is_empty() {
        true => None,
        false => {
            let decoded = MeshletMesh::decode(&mesh.meshlets)
                .map_err(|e| AssetError::Decode(format!("{path}: {e}")))?;
            Some(MeshletStats::of(&decoded, mesh.meshlets.len()))
        }
    };
    let missing = meshlets
        .is_none()
        .then(|| missing_reason(mesh, mesh_triangles, settings));
    Ok(MeshMeshlets {
        sub_asset: path.to_owned(),
        triangles: (mesh.indices.len() / 3) as u32,
        meshlets,
        missing,
    })
}

/// Why a mesh has no meshlets, from its import setting and content.
fn missing_reason(mesh: &MeshData, mesh_triangles: usize, settings: &serde_json::Value) -> String {
    if settings.get("meshlets") == Some(&serde_json::Value::Bool(false)) {
        return "meshlets: false".into();
    }
    if !mesh.joints.is_empty() || !mesh.weights.is_empty() {
        return "skinned (meshlets are for static meshes)".into();
    }
    let count = match mesh_triangles {
        1 => "1 triangle".to_owned(),
        n => format!("{n} triangles"),
    };
    format!("auto: the mesh has {count}, not more than {AUTO_MIN_TRIANGLES}")
}

/// Meshlet statistics of an imported model's meshes (meshlet counts, triangles and vertices per meshlet, bounds and normal cones), or why a mesh has no meshlets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssetMeshletStats;

/// `asset.meshlet_stats` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AssetMeshletStatsParams {
    /// "asset:<id>" for every mesh of a model, or "asset:<id>#mesh/<name>/<part>" for one.
    pub asset: String,
}

/// `asset.meshlet_stats` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssetMeshletStatsResult {
    /// Each mesh sub-asset, in sub-asset order.
    pub meshes: Vec<MeshMeshlets>,
}

impl Method for AssetMeshletStats {
    const NAME: &'static str = "asset.meshlet_stats";
    const MUTATES: bool = false;
    type Params = AssetMeshletStatsParams;
    type Result = AssetMeshletStatsResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: AssetMeshletStatsParams,
    ) -> Result<AssetMeshletStatsResult, ApiError> {
        let asset: AssetRef = p.asset.parse().map_err(|e: AssetError| api_error(e))?;
        Ok(AssetMeshletStatsResult {
            meshes: db(engine)?.meshlet_stats(&asset).map_err(api_error)?,
        })
    }
}
