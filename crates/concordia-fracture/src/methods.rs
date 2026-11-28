//! `fracture.generate` and `fracture.inspect`.

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method};
use concordia_assets::AssetDb;
use concordia_command::{Actor, EditOp};
use concordia_core::ddc::Ddc;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::{
    Breakable, EXTENSION, Fracture, FractureError, Pattern, Slot, Stored, StructureSpec, pieces,
};

/// Fracture a mesh into pieces from a seeded pattern and write the recipe as a `.breakable.ron` (e.g. {"mesh": "asset:<id>#mesh/Mesh/0", "pattern": {"Uniform": {"cells": 12}}, "seed": 1, "path": "breakables/wall.breakable.ron"}). Patterns: Uniform {cells}, Clustered {clusters, per_cluster, spread}, Radial {impact, cells, falloff}, Slices {normal, count}, Bricks {size}. The pieces are derived data in the DDC; the recipe is one undoable edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct FractureGenerate;

/// `fracture.generate` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct FractureGenerateParams {
    /// The source mesh ("asset:<id>#mesh/…"; asset.list shows sub-assets); it must be closed.
    pub mesh: String,
    /// How to divide it.
    pub pattern: Pattern,
    /// The pattern's seed (default 0).
    pub seed: Option<u64>,
    /// Project-relative output path ending in .breakable.ron.
    pub path: String,
    /// Metres per UV unit on interior faces (default 1).
    pub metres_per_tile: Option<f32>,
    /// The interior faces' material (default: the source's).
    pub interior_material: Option<String>,
    /// Density, bond strength, cluster levels and edge overrides (default: stone, no levels).
    pub structure: Option<StructureSpec>,
    /// Who is fracturing (required; logged).
    pub actor: Option<Actor>,
}

/// One piece, summarised.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PieceSummary {
    /// The pattern cell it came from.
    pub cell: u32,
    /// Its triangles.
    pub triangles: u32,
    /// Those on a cut (interior material).
    pub interior_triangles: u32,
    /// Its volume (cubic metres).
    pub volume: f64,
}

/// `fracture.generate` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct FractureGenerateResult {
    /// The recipe written.
    pub path: String,
    /// The pieces, in cell order.
    pub pieces: Vec<PieceSummary>,
    /// The pieces' DDC key.
    pub cache_key: String,
}

/// Summarise a `.breakable.ron`'s pieces, rebuilding them from the recipe if the DDC lacks them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct FractureInspect;

/// `fracture.inspect` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct FractureInspectParams {
    /// Project-relative .breakable.ron.
    pub path: String,
}

/// `fracture.inspect` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct FractureInspectResult {
    /// The recipe.
    pub recipe: Breakable,
    /// The pieces, in cell order.
    pub pieces: Vec<PieceSummary>,
    /// The pieces' DDC key.
    pub cache_key: String,
    /// They were read from the DDC rather than cut again.
    pub cached: bool,
}

impl Method for FractureGenerate {
    const NAME: &'static str = "fracture.generate";
    const MUTATES: bool = true;
    type Params = FractureGenerateParams;
    type Result = FractureGenerateResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: FractureGenerateParams,
    ) -> Result<FractureGenerateResult, ApiError> {
        engine.project_file(&p.path, EXTENSION)?;
        let recipe = Breakable {
            source: p.mesh,
            pattern: p.pattern,
            seed: p.seed.unwrap_or(0),
            metres_per_tile: p.metres_per_tile.unwrap_or(1.0),
            interior_material: p.interior_material,
            structure: p.structure.unwrap_or_default(),
        };
        if !(recipe.metres_per_tile.is_finite() && recipe.metres_per_tile > 0.0) {
            return Err(
                invalid("`metres_per_tile` must be a positive number").at("/metres_per_tile")
            );
        }
        let stored = cut(engine, &recipe)?;
        let after = concordia_scene::write_document(&recipe).map_err(|e| invalid(e.to_string()))?;
        let before = read(engine, &p.path)?;
        if before.as_deref() != Some(after.as_str()) {
            let op = EditOp::Document {
                path: p.path.clone(),
                before,
                after: Some(after),
            };
            engine.commit_edits(ctx, "fracture.generate", vec![op])?;
        }
        Ok(FractureGenerateResult {
            path: p.path,
            pieces: summary(&stored.fracture),
            cache_key: stored.key,
        })
    }
}

impl Method for FractureInspect {
    const NAME: &'static str = "fracture.inspect";
    const MUTATES: bool = false;
    type Params = FractureInspectParams;
    type Result = FractureInspectResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: FractureInspectParams,
    ) -> Result<FractureInspectResult, ApiError> {
        let text = read(engine, &p.path)?
            .ok_or_else(|| invalid(format!("no breakable at {}", p.path)).at("/path"))?;
        let recipe: Breakable = concordia_scene::read_document(&text)
            .map_err(|e| invalid(format!("{}: {e}", p.path)).at("/path"))?;
        let stored = cut(engine, &recipe)?;
        Ok(FractureInspectResult {
            recipe,
            pieces: summary(&stored.fracture),
            cache_key: stored.key,
            cached: stored.cached,
        })
    }
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message)
}

/// The recipe's pieces, through the project's DDC.
fn cut(engine: &mut Engine, recipe: &Breakable) -> Result<Stored, ApiError> {
    let ddc = Ddc::new(engine.project().join(".concordia/ddc"));
    let db = engine.state_mut::<AssetDb>().ok_or_else(|| {
        ApiError::new(
            ApiErrorKind::Internal,
            "fracture needs [assets] (the asset database failed to load; see log.tail)",
        )
    })?;
    pieces(recipe, db, &ddc).map_err(api_error)
}

fn api_error(e: FractureError) -> ApiError {
    match &e {
        FractureError::InvalidMesh(_) | FractureError::NotClosed => {
            invalid(e.to_string()).at("/mesh")
        }
        FractureError::InvalidPattern(_) => invalid(e.to_string()).at("/pattern"),
        FractureError::InvalidParameter(_) => invalid(e.to_string()),
        FractureError::Hull(_) | FractureError::Io(_) | FractureError::Cutter => {
            ApiError::new(ApiErrorKind::Internal, e.to_string())
        }
    }
}

fn read(engine: &Engine, path: &str) -> Result<Option<String>, ApiError> {
    let file = engine.project_file(path, EXTENSION)?;
    match std::fs::read_to_string(&file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ApiError::new(ApiErrorKind::SceneIo, format!("{path}: {e}"))),
    }
}

fn summary(f: &Fracture) -> Vec<PieceSummary> {
    f.pieces
        .iter()
        .map(|p| PieceSummary {
            cell: p.cell,
            triangles: p.triangles.len() as u32,
            interior_triangles: p.materials.iter().filter(|s| **s == Slot::Interior).count() as u32,
            volume: p.volume,
        })
        .collect()
}
