//! Gathering a volume's input from a world: static colliders, area marks and settings, all in
//! entity order so the input doesn't depend on how the world was filled.

use concordia_core::ddc::DdcKey;
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_physics::static_solids;
use concordia_scene::builtin::Transform;
use concordia_terra::World;
use recast_sys::{Area, BoxMark, RECAST_VERSION};

use crate::components::{NavArea, NavAreaGrid, NavAreaKind, NavMeshVolume};
use crate::error::NavError;
use crate::settings::NavSettings;
use crate::tessellate::triangles;

/// Distinct costs a world may use (Recast area ids 2..=62).
const MAX_COSTS: usize = (Area::LAST_COST - Area::FIRST_COST + 1) as usize;

/// Everything a navmesh is built from, in a canonical order.
#[derive(Debug, Clone, PartialEq)]
pub struct NavInput {
    /// The volume.
    pub volume: EntityId,
    /// Build settings.
    pub settings: NavSettings,
    /// The volume's bounds.
    pub min: Vec3,
    /// The volume's bounds.
    pub max: Vec3,
    /// Triangle vertices (world space).
    pub vertices: Vec<[f32; 3]>,
    /// Triangles, facing outward from their solids.
    pub triangles: Vec<[u32; 3]>,
    /// Area marks in order: `NavArea`s, then `NavAreaGrid`s, each by entity id.
    pub marks: Vec<BoxMark>,
    /// Travel cost of each cost area id.
    pub costs: Vec<(u8, f32)>,
}

impl NavInput {
    /// Gathers `volume`'s input from `world`.
    pub fn gather(world: &World, volume: EntityId) -> Result<Self, NavError> {
        let (Some(v), Some(t)) = (
            world.get::<NavMeshVolume>(volume),
            world.get::<Transform>(volume),
        ) else {
            return Err(NavError::NoVolume(volume));
        };
        let settings = NavSettings::from_volume(volume, v)?;
        let (min, max) = (
            t.translation - v.half_extents,
            t.translation + v.half_extents,
        );
        let mut input = Self {
            volume,
            settings,
            min,
            max,
            vertices: Vec::new(),
            triangles: Vec::new(),
            marks: Vec::new(),
            costs: Vec::new(),
        };
        input.add_solids(world);
        let areas = areas(world)?;
        input.costs = cost_ids(&areas)?;
        for (_, center, area) in &areas {
            let id = match area.kind {
                NavAreaKind::Blocked => Area::BLOCKED,
                NavAreaKind::Cost { cost } => input.cost_area(cost),
            };
            input.push_mark(*center - area.half_extents, *center + area.half_extents, id);
        }
        for (entity, corner, grid) in grids(world) {
            input.add_grid(entity, corner, grid)?;
        }
        Ok(input)
    }

    /// The cache key of this input (format, Recast version, settings, geometry and marks).
    pub fn key(&self) -> DdcKey {
        let c = &self.settings.config;
        let floats = |xs: &[f32]| xs.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let config = floats(&[
            c.cell_size,
            c.cell_height,
            c.agent_height,
            c.agent_radius,
            c.agent_max_climb,
            c.agent_max_slope_deg,
            c.tile_size as f32,
            c.region_min_size as f32,
            c.region_merge_size as f32,
            c.edge_max_len,
            c.edge_max_error,
            c.verts_per_poly as f32,
            c.detail_sample_dist,
            c.detail_sample_max_error,
        ]);
        let bounds = floats(&[self.min.to_array(), self.max.to_array()].concat());
        let vertices = floats(self.vertices.as_flattened());
        let triangles: Vec<u8> = self
            .triangles
            .as_flattened()
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect();
        let marks: Vec<u8> = self
            .marks
            .iter()
            .flat_map(|m| {
                let mut b = floats(&[m.min, m.max].concat());
                b.push(m.area.id());
                b
            })
            .collect();
        let costs: Vec<u8> = self
            .costs
            .iter()
            .flat_map(|(id, cost)| [vec![*id], cost.to_le_bytes().to_vec()].concat())
            .collect();
        DdcKey::of(&[
            b"concordia-nav/1",
            RECAST_VERSION.as_bytes(),
            &config,
            &bounds,
            &vertices,
            &triangles,
            &marks,
            &costs,
        ])
    }

    fn add_solids(&mut self, world: &World) {
        for solid in static_solids(world) {
            for tri in triangles(&solid.collider.shape) {
                let w =
                    tri.map(|p| solid.translation + solid.rotation * (p + solid.collider.offset));
                let lo = w[0].min(w[1]).min(w[2]);
                let hi = w[0].max(w[1]).max(w[2]);
                if lo.cmple(self.max).all() && hi.cmpge(self.min).all() {
                    let base = self.vertices.len() as u32;
                    self.vertices.extend(w.map(|p| p.to_array()));
                    self.triangles.push([base, base + 1, base + 2]);
                }
            }
        }
    }

    fn cost_area(&self, cost: f32) -> Area {
        let found = self.costs.iter().find(|(_, c)| c.total_cmp(&cost).is_eq());
        found
            .and_then(|(id, _)| Area::cost(*id))
            .unwrap_or(Area::WALKABLE)
    }

    /// Keeps marks that touch the volume. Upper bounds are exclusive (Recast's box marks include
    /// the cell an upper bound lands on), so a box ending on a cell boundary marks no cell past it.
    fn push_mark(&mut self, min: Vec3, max: Vec3, area: Area) {
        let inset = Vec3::new(
            self.settings.config.cell_size,
            0.0,
            self.settings.config.cell_size,
        ) * 1e-3;
        let max = (max - inset).max(min);
        if min.cmple(self.max).all() && max.cmpge(self.min).all() {
            self.marks.push(BoxMark {
                min: min.to_array(),
                max: max.to_array(),
                area,
            });
        }
    }

    fn add_grid(
        &mut self,
        entity: EntityId,
        corner: Vec3,
        grid: &NavAreaGrid,
    ) -> Result<(), NavError> {
        let cells = parse_grid(grid).map_err(|message| NavError::Invalid { entity, message })?;
        let s = grid.cell_size;
        for (r, row) in cells.iter().enumerate() {
            for (start, end, area) in runs(row) {
                let lo = Vec3::new(
                    corner.x + start as f32 * s,
                    self.min.y,
                    corner.z + r as f32 * s,
                );
                let hi = Vec3::new(
                    corner.x + end as f32 * s,
                    self.max.y,
                    corner.z + (r + 1) as f32 * s,
                );
                self.push_mark(lo, hi, area);
            }
        }
        Ok(())
    }
}

/// Every `NavArea` with its centre, checked, by entity id.
fn areas(world: &World) -> Result<Vec<(EntityId, Vec3, &NavArea)>, NavError> {
    let mut out = Vec::new();
    for entity in world.entity_ids_sorted() {
        let (Some(area), Some(t)) = (world.get::<NavArea>(entity), world.get::<Transform>(entity))
        else {
            continue;
        };
        check_area(area).map_err(|message| NavError::Invalid { entity, message })?;
        out.push((entity, t.translation, area));
    }
    Ok(out)
}

fn check_area(area: &NavArea) -> Result<(), String> {
    if !area
        .half_extents
        .to_array()
        .iter()
        .all(|x| x.is_finite() && *x >= 0.0)
    {
        return Err(format!(
            "NavArea.half_extents must be numbers ≥ 0, got {}",
            area.half_extents
        ));
    }
    match area.kind {
        NavAreaKind::Cost { cost } if !(cost.is_finite() && cost >= 1.0) => {
            Err(format!("NavArea cost must be a number ≥ 1, got {cost}"))
        }
        _ => Ok(()),
    }
}

/// One area id per distinct cost, in ascending cost order.
fn cost_ids(areas: &[(EntityId, Vec3, &NavArea)]) -> Result<Vec<(u8, f32)>, NavError> {
    let mut costs: Vec<f32> = areas
        .iter()
        .filter_map(|(_, _, a)| match a.kind {
            NavAreaKind::Cost { cost } => Some(cost),
            NavAreaKind::Blocked => None,
        })
        .collect();
    costs.sort_by(f32::total_cmp);
    costs.dedup_by(|a, b| a.total_cmp(b).is_eq());
    if costs.len() > MAX_COSTS {
        return Err(NavError::TooManyCosts {
            count: costs.len(),
            max: MAX_COSTS,
        });
    }
    Ok(costs
        .into_iter()
        .enumerate()
        .map(|(i, c)| (Area::FIRST_COST + i as u8, c))
        .collect())
}

/// Every `NavAreaGrid` with its corner, by entity id.
fn grids(world: &World) -> Vec<(EntityId, Vec3, &NavAreaGrid)> {
    world
        .entity_ids_sorted()
        .into_iter()
        .filter_map(|e| {
            Some((
                e,
                world.get::<Transform>(e)?.translation,
                world.get::<NavAreaGrid>(e)?,
            ))
        })
        .collect()
}

/// A grid's cells: `None` keeps, `Some(area)` marks.
fn parse_grid(grid: &NavAreaGrid) -> Result<Vec<Vec<Option<Area>>>, String> {
    if !(grid.cell_size.is_finite() && grid.cell_size > 0.0) {
        return Err(format!(
            "NavAreaGrid.cell_size must be a positive number, got {}",
            grid.cell_size
        ));
    }
    let width = grid.rows.first().map_or(0, |r| r.chars().count());
    grid.rows
        .iter()
        .enumerate()
        .map(|(r, row)| {
            let n = row.chars().count();
            if n != width {
                return Err(format!("NavAreaGrid.rows must all be {width} cells wide; row {r} has {n}"));
            }
            row.chars()
                .enumerate()
                .map(|(c, ch)| match ch {
                    '.' => Ok(None),
                    '#' => Ok(Some(Area::BLOCKED)),
                    'o' => Ok(Some(Area::WALKABLE)),
                    other => Err(format!("NavAreaGrid.rows: unknown cell {other:?} at row {r}, column {c} (use '.', '#' or 'o')")),
                })
                .collect()
        })
        .collect()
}

/// Runs of equal marks in a row: (first column, one past the last, area).
fn runs(row: &[Option<Area>]) -> Vec<(usize, usize, Area)> {
    let mut out: Vec<(usize, usize, Area)> = Vec::new();
    for (c, cell) in row.iter().enumerate() {
        let Some(area) = *cell else { continue };
        match out.last_mut() {
            Some(last) if last.1 == c && last.2 == area => last.1 = c + 1,
            _ => out.push((c, c + 1, area)),
        }
    }
    out
}
