//! Navmeshes cached in the DDC by their input's key.

use std::num::NonZeroUsize;

use concordia_core::ddc::Ddc;

use crate::build::build_tiles;
use crate::data::NavMeshData;
use crate::error::NavError;
use crate::gather::NavInput;
use crate::navmesh::NavMesh;

/// A navmesh and whether it came from the cache.
#[derive(Debug)]
pub struct Loaded {
    /// The navmesh.
    pub navmesh: NavMesh,
    /// Read from the DDC rather than built.
    pub cached: bool,
}

/// Builds navmeshes, or reads them from the DDC when their input is unchanged.
#[derive(Debug, Clone)]
pub struct NavCache {
    ddc: Ddc,
}

impl NavCache {
    /// A cache over `ddc`.
    pub fn new(ddc: Ddc) -> Self {
        Self { ddc }
    }

    /// The navmesh for `input`: from the DDC if present and readable, else built and stored.
    pub fn load_or_build(
        &self,
        input: &NavInput,
        threads: NonZeroUsize,
    ) -> Result<Loaded, NavError> {
        let key = input.key();
        if let Some(bytes) = self
            .ddc
            .get(&key)
            .map_err(|e| NavError::Cache(e.to_string()))?
        {
            match NavMeshData::from_bytes(&bytes).and_then(NavMesh::from_data) {
                Ok(navmesh) => {
                    return Ok(Loaded {
                        navmesh,
                        cached: true,
                    });
                }
                Err(e) => {
                    tracing::warn!(target: "concordia::nav", %key, error = %e, "cached navmesh unreadable; rebuilding")
                }
            }
        }
        let data = build_tiles(input, threads)?;
        self.ddc
            .put(&key, &data.to_bytes())
            .map_err(|e| NavError::Cache(e.to_string()))?;
        Ok(Loaded {
            navmesh: NavMesh::from_data(data)?,
            cached: false,
        })
    }
}
