//! `material.get` and `material.set`: read and write `.material.ron` files as canonical text
//! (M2 phase 2.3c, ADR-0068). File writes are attributed in `log.tail`, never journaled (D9).

use std::path::{Path, PathBuf};

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method};
use concordia_command::Actor;
use concordia_core::id::AssetId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::material::{
    Material, MaterialError, MaterialFormat, MaterialNode, read_material_with_format,
    write_material,
};
use crate::module::{api_error, db, publish_asset};

/// Read a `.material.ron` file as a layered material (M1's flat files read as one slab, `migrated: true`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MaterialGet;

/// `material.get` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct MaterialGetParams {
    /// Project-relative path, e.g. "assets/materials/steel.material.ron".
    pub path: String,
}

/// `material.get` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MaterialGetResult {
    /// The material.
    pub material: Material,
    /// True if the file is M1's flat format (saving rewrites it as layered text).
    pub migrated: bool,
    /// The asset id, if the file has been imported.
    pub id: Option<AssetId>,
}

impl Method for MaterialGet {
    const NAME: &'static str = "material.get";
    const MUTATES: bool = false;
    type Params = MaterialGetParams;
    type Result = MaterialGetResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: MaterialGetParams,
    ) -> Result<MaterialGetResult, ApiError> {
        let rel = material_path(&p.path)?;
        let abs = engine.project().join(&rel);
        let text = std::fs::read_to_string(&abs).map_err(|e| {
            ApiError::new(
                ApiErrorKind::InvalidParams,
                format!("cannot read material {}: {e}", p.path),
            )
        })?;
        let (material, format) = read_material_with_format(&text)
            .map_err(|e| ApiError::new(ApiErrorKind::SceneParse, format!("{}: {e}", p.path)))?;
        Ok(MaterialGetResult {
            material,
            migrated: format == MaterialFormat::M1,
            id: db(engine)?.id_at(&rel),
        })
    }
}

/// Write a material to a `.material.ron` file (created if missing) as canonical text and import it, so the renderer redraws it. Invalid materials are refused with the node and field. Attributed in log.tail; not undoable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MaterialSet;

/// `material.set` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct MaterialSetParams {
    /// Project-relative path ending in ".material.ron".
    pub path: String,
    /// The whole material (read it with material.get, change it, send it back).
    pub material: Material,
    /// Who is writing (attribution in the log).
    pub actor: Option<Actor>,
}

/// `material.set` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MaterialSetResult {
    /// Asset id (refer to it as "asset:<id>").
    pub id: AssetId,
    /// False if the file already held this text.
    pub written: bool,
}

impl Method for MaterialSet {
    const NAME: &'static str = "material.set";
    const MUTATES: bool = false;
    type Params = MaterialSetParams;
    type Result = MaterialSetResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: MaterialSetParams,
    ) -> Result<MaterialSetResult, ApiError> {
        let rel = material_path(&p.path)?;
        let text = write_material(&p.material).map_err(|e| refused(&e, &p.material))?;
        let abs = engine.project().join(&rel);
        let written = std::fs::read_to_string(&abs).ok().as_deref() != Some(text.as_str());
        if written {
            write_file(&abs, &text)?;
        }
        let id = db(engine)?.import(&rel).map_err(api_error)?.id;
        let actor = ctx
            .actor
            .as_ref()
            .map_or_else(|| "none".into(), Actor::to_string);
        tracing::info!(target: "concordia::assets", actor, path = %p.path, id = %id, written, "material saved");
        publish_asset(engine, &p.path, id, &actor);
        Ok(MaterialSetResult { id, written })
    }
}

/// A project-relative `.material.ron` path.
fn material_path(path: &str) -> Result<PathBuf, ApiError> {
    if !path.ends_with(".material.ron") {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("{path:?} is not a .material.ron file"),
        ));
    }
    crate::db::relative(Path::new(path)).map_err(api_error)
}

fn write_file(abs: &Path, text: &str) -> Result<(), ApiError> {
    let io =
        |e: std::io::Error| ApiError::new(ApiErrorKind::SceneIo, format!("{}: {e}", abs.display()));
    if let Some(dir) = abs.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    std::fs::write(abs, text).map_err(io)
}

/// An invalid material, with the node, the `node.field` and a pointer into the params.
fn refused(e: &MaterialError, material: &Material) -> ApiError {
    let mut err = ApiError::new(ApiErrorKind::InvalidParams, format!("material: {e}"))
        .see("concordia://schema/method/material.set");
    if let Some(field) = e.field() {
        err = err.with("field", json!(field));
    }
    if let Some(node) = e.node() {
        err = err.with("node", json!(node));
        if let Some(pointer) = pointer(material, node, e.field()) {
            err = err.at(pointer);
        }
    }
    err
}

/// `/material/nodes/<node>/<Variant>[/<field path>]` for `node.field[i]`.
fn pointer(material: &Material, node: &str, field: Option<&str>) -> Option<String> {
    let variant = match material.node(node)? {
        MaterialNode::Slab(_) => "Slab",
        MaterialNode::Vertical { .. } => "Vertical",
        MaterialNode::Horizontal { .. } => "Horizontal",
        MaterialNode::Coverage { .. } => "Coverage",
    };
    let mut out = format!("/material/nodes/{node}/{variant}");
    if let Some(rest) = field.and_then(|f| f.split_once('.')).map(|(_, r)| r) {
        let path = rest.replace(['.', '['], "/").replace(']', "");
        out.push('/');
        out.push_str(&path);
    }
    Some(out)
}
