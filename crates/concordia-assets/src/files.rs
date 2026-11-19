//! `project.files`: the project's files with their kind and import status, for the editor's
//! FileSystem dock and for agents (M1 phase 1.11).

use std::collections::HashMap;
use std::path::{Component, Path};

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method};
use concordia_core::id::AssetId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::db::AssetDb;
use crate::module::db;
use crate::sidecar::Sidecar;

/// List the project's files (project-relative, sorted) with kind, size and import status. Skips hidden and git-ignored paths, engine state and import sidecars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ProjectFiles;

/// `project.files` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ProjectFilesParams {
    /// Only files under this project-relative directory (default: the whole project).
    #[serde(default)]
    pub dir: Option<String>,
}

/// What a file is, by name.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum FileKind {
    /// A `.scene.ron` scene.
    scene,
    /// A `.prefab.ron` prefab.
    prefab,
    /// A glTF model (`.gltf`, `.glb`).
    model,
    /// An image (`.png`, `.jpg`, `.jpeg`, `.hdr`).
    image,
    /// A `.material.ron` material.
    material,
    /// Anything else.
    other,
}

/// One project file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ProjectFile {
    /// Project-relative path with `/` separators.
    pub path: String,
    /// What it is.
    pub kind: FileKind,
    /// Size in bytes.
    pub size: u64,
    /// The asset id if it has been imported.
    pub asset: Option<AssetId>,
    /// The importer that handles it, if any (null: it cannot be imported).
    pub importer: Option<String>,
}

/// `project.files` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ProjectFilesResult {
    /// Files sorted by path.
    pub files: Vec<ProjectFile>,
}

impl Method for ProjectFiles {
    const NAME: &'static str = "project.files";
    const MUTATES: bool = false;
    type Params = ProjectFilesParams;
    type Result = ProjectFilesResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ProjectFilesParams,
    ) -> Result<ProjectFilesResult, ApiError> {
        let root = engine.project().to_path_buf();
        let dir = match &p.dir {
            Some(dir) => checked_dir(&root, dir)?,
            None => root.clone(),
        };
        let db = db(engine)?;
        let imported: HashMap<String, AssetId> =
            db.list().into_iter().map(|a| (a.path, a.id)).collect();
        let mut files: Vec<ProjectFile> = walk(&root, &dir)
            .into_iter()
            .map(|(rel, size)| describe(db, &imported, rel, size))
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(ProjectFilesResult { files })
    }
}

/// `dir` resolved under `root`; it must be relative, stay inside, and exist.
fn checked_dir(root: &Path, dir: &str) -> Result<std::path::PathBuf, ApiError> {
    let rel = Path::new(dir);
    let escapes = rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)));
    let full = root.join(rel);
    match (escapes, full.is_dir()) {
        (true, _) => Err(invalid(format!(
            "{dir:?} must be a relative path inside the project"
        ))),
        (false, false) => Err(invalid(format!(
            "{dir:?} is not a directory in the project"
        ))),
        (false, true) => Ok(full),
    }
}

/// Project-relative paths and sizes of the visible files under `dir`.
fn walk(root: &Path, dir: &Path) -> Vec<(String, u64)> {
    ignore::WalkBuilder::new(dir)
        .hidden(true)
        .require_git(false)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            let rel = e.path().strip_prefix(root).ok()?;
            let text = rel.to_str()?.replace('\\', "/");
            let size = e.metadata().map_or(0, |m| m.len());
            (!text.ends_with(Sidecar::SUFFIX)).then_some((text, size))
        })
        .collect()
}

fn describe(
    db: &AssetDb,
    imported: &HashMap<String, AssetId>,
    path: String,
    size: u64,
) -> ProjectFile {
    ProjectFile {
        kind: kind_of(&path),
        size,
        asset: imported.get(&path).copied(),
        importer: db.importer_name(Path::new(&path)).map(str::to_owned),
        path,
    }
}

/// A file's kind from its name.
fn kind_of(path: &str) -> FileKind {
    let lower = path.to_ascii_lowercase();
    let ends = |suffixes: &[&str]| suffixes.iter().any(|s| lower.ends_with(s));
    match () {
        _ if ends(&[".scene.ron"]) => FileKind::scene,
        _ if ends(&[".prefab.ron"]) => FileKind::prefab,
        _ if ends(&[".gltf", ".glb"]) => FileKind::model,
        _ if ends(&[".png", ".jpg", ".jpeg", ".hdr"]) => FileKind::image,
        _ if ends(&[".material.ron"]) => FileKind::material,
        _ => FileKind::other,
    }
}

fn invalid(message: String) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message).at("/dir")
}
