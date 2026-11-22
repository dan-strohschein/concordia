//! Validated build settings from a [`NavMeshVolume`].

use concordia_core::id::EntityId;
use recast_sys::BuildConfig;

use crate::components::NavMeshVolume;
use crate::error::NavError;

/// A volume's settings, checked, as Recast's build configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavSettings {
    /// What Recast builds with.
    pub config: BuildConfig,
}

impl NavSettings {
    /// Checks `volume` (on `entity`) and derives Recast's configuration.
    pub fn from_volume(entity: EntityId, volume: &NavMeshVolume) -> Result<Self, NavError> {
        check(volume).map_err(|message| NavError::Invalid { entity, message })?;
        Ok(Self {
            config: config(volume),
        })
    }

    /// One tile's side in meters.
    pub fn tile_width(&self) -> f32 {
        self.config.tile_size as f32 * self.config.cell_size
    }
}

fn check(v: &NavMeshVolume) -> Result<(), String> {
    let positive = |name: &str, x: f32| match x.is_finite() && x > 0.0 {
        true => Ok(()),
        false => Err(format!(
            "NavMeshVolume.{name} must be a positive number, got {x}"
        )),
    };
    let at_least_zero = |name: &str, x: f32| match x.is_finite() && x >= 0.0 {
        true => Ok(()),
        false => Err(format!(
            "NavMeshVolume.{name} must be a number ≥ 0, got {x}"
        )),
    };
    for x in v.half_extents.to_array() {
        positive("half_extents", x)?;
    }
    positive("cell_size", v.cell_size)?;
    positive("cell_height", v.cell_height)?;
    at_least_zero("agent_radius", v.agent_radius)?;
    positive("agent_height", v.agent_height)?;
    at_least_zero("agent_max_climb", v.agent_max_climb)?;
    if !(v.agent_max_slope_deg.is_finite()
        && v.agent_max_slope_deg > 0.0
        && v.agent_max_slope_deg < 90.0)
    {
        return Err(format!(
            "NavMeshVolume.agent_max_slope_deg must be in (0, 90), got {}",
            v.agent_max_slope_deg
        ));
    }
    if !(v.edge_max_error.is_finite() && (0.0..=3.0).contains(&v.edge_max_error)) {
        return Err(format!(
            "NavMeshVolume.edge_max_error must be 0..=3 cells, got {}",
            v.edge_max_error
        ));
    }
    if !(8..=1024).contains(&v.tile_size) {
        return Err(format!(
            "NavMeshVolume.tile_size must be 8..=1024 cells, got {}",
            v.tile_size
        ));
    }
    Ok(())
}

fn config(v: &NavMeshVolume) -> BuildConfig {
    BuildConfig {
        cell_size: v.cell_size,
        cell_height: v.cell_height,
        agent_height: v.agent_height,
        agent_radius: v.agent_radius,
        agent_max_climb: v.agent_max_climb,
        agent_max_slope_deg: v.agent_max_slope_deg,
        tile_size: v.tile_size,
        region_min_size: 8,
        region_merge_size: 20,
        edge_max_len: 12.0,
        edge_max_error: v.edge_max_error,
        verts_per_poly: 6,
        detail_sample_dist: 6.0 * v.cell_size,
        detail_sample_max_error: v.cell_height,
    }
}
