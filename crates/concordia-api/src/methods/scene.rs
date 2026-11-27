//! `scene.new`, `scene.open`, `scene.save`, `scene.info`.

use std::path::{Path, PathBuf};

use concordia_command::{Actor, Author, CommandLog, History, Journal};
use concordia_core::id::{IdSource, SceneId};
use concordia_reflect::Reflect;
use concordia_scene::{SceneMeta, load_into_world, read_scene, snapshot_world, write_scene};
use concordia_terra::World;
use serde::{Deserialize, Serialize};

use super::conflict;
use crate::engine::{Engine, OpenScene, internal};
use crate::error::{ApiError, ApiErrorKind};
use crate::map::{self, Hints};
use crate::method::{CallCtx, Method, MethodInfo};

/// Create a new empty scene at a project-relative path (written on scene.save).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneNew;

/// `scene.new` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SceneNewParams {
    /// Project-relative path ending in `.scene.ron`, e.g. `scenes/arena.scene.ron`.
    pub path: String,
    /// Display name.
    pub name: String,
    /// Throw away unsaved changes in the currently open scene.
    pub discard: Option<bool>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// `scene.new` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneNewResult {
    /// The new scene's id.
    pub scene_id: SceneId,
    /// Its path.
    pub path: String,
}

/// Open a scene file; its undo history is restored if the journal matches the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneOpen;

/// `scene.open` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SceneOpenParams {
    /// Project-relative path to a `.scene.ron` file.
    pub path: String,
    /// Throw away unsaved changes in the currently open scene.
    pub discard: Option<bool>,
}

/// `scene.open` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneOpenResult {
    /// Scene id.
    pub scene_id: SceneId,
    /// Number of entities.
    pub entity_count: u64,
    /// "restored", "stale" (file changed outside the engine; undo unavailable) or "none".
    pub history: String,
}

/// Write the open scene in canonical form (to its path, or to a new path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneSave;

/// `scene.save` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SceneSaveParams {
    /// Save to this project-relative path instead (becomes the scene's path).
    pub path: Option<String>,
    /// Actor recorded in the journal (defaults to the session's, else system:engine).
    pub actor: Option<Actor>,
}

/// `scene.save` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneSaveResult {
    /// Path written.
    pub path: String,
    /// Bytes written.
    pub bytes: u64,
    /// World hash at save.
    pub hash: String,
}

/// Describe the open scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneInfo;

/// `scene.info` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneInfoResult {
    /// Scene id.
    pub scene_id: SceneId,
    /// Display name.
    pub name: String,
    /// Project-relative path.
    pub path: String,
    /// True if there are unsaved changes.
    pub dirty: bool,
    /// Number of entities.
    pub entity_count: u64,
    /// Component names used by at least one entity, sorted.
    pub components_in_use: Vec<String>,
}

/// The open scene as canonical RON, exactly as `scene.save` would write it, without saving.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneText;

/// `scene.text` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SceneTextResult {
    /// Project-relative path of the open scene.
    pub path: String,
    /// The scene file's text.
    pub text: String,
}

impl Method for SceneNew {
    const NAME: &'static str = "scene.new";
    const MUTATES: bool = true;
    type Params = SceneNewParams;
    type Result = SceneNewResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: SceneNewParams,
    ) -> Result<SceneNewResult, ApiError> {
        ensure_replaceable(engine, ctx, p.discard)?;
        let file = project_path(engine, &p.path)?;
        if file.exists() {
            return Err(
                ApiError::new(ApiErrorKind::SceneIo, format!("{} already exists", p.path)).suggest(
                    [format!(
                        "open it with scene.open {{\"path\": \"{}\"}}",
                        p.path
                    )],
                ),
            );
        }
        let id: SceneId = engine.ids.next_id().map_err(internal)?;
        let (journal, _) = open_journal(engine, id)?;
        let mut log = CommandLog::new();
        log.attach_journal(journal);
        log.attach_documents(project_documents(engine));
        let meta = SceneMeta { id, name: p.name };
        engine.play = None;
        engine.scene = Some(OpenScene {
            meta,
            path: p.path.clone(),
            world: World::new(),
            log,
            dirty: false,
            hold: None,
        });
        Ok(SceneNewResult {
            scene_id: id,
            path: p.path,
        })
    }
}

impl Method for SceneOpen {
    const NAME: &'static str = "scene.open";
    const MUTATES: bool = false;
    type Params = SceneOpenParams;
    type Result = SceneOpenResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: SceneOpenParams,
    ) -> Result<SceneOpenResult, ApiError> {
        ensure_replaceable(engine, ctx, p.discard)?;
        let file = project_path(engine, &p.path)?;
        let text = std::fs::read_to_string(&file).map_err(|e| map::io(&p.path, &e))?;
        let hints = Hints {
            registry: &engine.registry,
            world: None,
        };
        let doc = read_scene(&text, &engine.registry).map_err(|e| map::scene(e, &hints))?;
        let mut world = World::new();
        load_into_world(&doc, &mut world, &engine.registry).map_err(|e| map::scene(e, &hints))?;
        let (journal, loaded) = open_journal(engine, doc.id)?;
        let (mut log, history) =
            CommandLog::resume(&loaded.entries, world.state_hash().map_err(internal)?);
        log.attach_journal(journal);
        log.attach_documents(project_documents(engine));
        let history = match history {
            History::Restored => "restored",
            History::Stale { .. } => "stale",
            History::None => "none",
        };
        let entity_count = world.len() as u64;
        tracing::info!(target: "concordia::scene", path = %p.path, entities = entity_count, history, "scene opened");
        engine.play = None;
        engine.scene = Some(OpenScene {
            meta: doc.meta(),
            path: p.path,
            world,
            log,
            dirty: false,
            hold: None,
        });
        Ok(SceneOpenResult {
            scene_id: doc.id,
            entity_count,
            history: history.to_owned(),
        })
    }
}

impl Method for SceneSave {
    const NAME: &'static str = "scene.save";
    const MUTATES: bool = false;
    type Params = SceneSaveParams;
    type Result = SceneSaveResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: SceneSaveParams,
    ) -> Result<SceneSaveResult, ApiError> {
        if let Some(err) = conflict(engine.scene()?, ctx) {
            return Err(
                err.suggest(["commit or abort the open transaction before saving".to_owned()])
            );
        }
        let path = p
            .path
            .unwrap_or_else(|| engine.scene().map(|s| s.path.clone()).unwrap_or_default());
        let file = project_path(engine, &path)?;
        let text = engine.scene_text()?;
        write_atomically(&file, &text).map_err(|e| map::io(&path, &e))?;
        let hash = engine.scene()?.world.state_hash().map_err(internal)?;
        let actor = ctx
            .actor
            .clone()
            .unwrap_or_else(|| "system:engine".parse().expect("valid literal actor"));
        let scene = engine.scene_mut()?;
        scene
            .log
            .record_save(
                Author {
                    actor,
                    session: Some(ctx.session),
                },
                &path,
                hash,
            )
            .map_err(|e| ApiError::new(ApiErrorKind::SceneIo, e.to_string()))?;
        scene.path = path.clone();
        scene.dirty = false;
        tracing::info!(target: "concordia::scene", path = %path, bytes = text.len(), "scene saved");
        Ok(SceneSaveResult {
            path,
            bytes: text.len() as u64,
            hash: hash.to_string(),
        })
    }
}

impl Method for SceneInfo {
    const NAME: &'static str = "scene.info";
    const MUTATES: bool = false;
    type Params = super::Empty;
    type Result = SceneInfoResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        _: super::Empty,
    ) -> Result<SceneInfoResult, ApiError> {
        let scene = engine.scene()?;
        let mut in_use: Vec<String> = scene
            .world
            .entity_ids_sorted()
            .into_iter()
            .flat_map(|id| scene.world.component_names(id).unwrap_or_default())
            .map(str::to_owned)
            .collect();
        in_use.sort_unstable();
        in_use.dedup();
        Ok(SceneInfoResult {
            scene_id: scene.meta.id,
            name: scene.meta.name.clone(),
            path: scene.path.clone(),
            dirty: scene.dirty,
            entity_count: scene.world.len() as u64,
            components_in_use: in_use,
        })
    }
}

impl Method for SceneText {
    const NAME: &'static str = "scene.text";
    const MUTATES: bool = false;
    type Params = super::Empty;
    type Result = SceneTextResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        _: super::Empty,
    ) -> Result<SceneTextResult, ApiError> {
        Ok(SceneTextResult {
            path: engine.scene()?.path.clone(),
            text: engine.scene_text()?,
        })
    }
}

impl Engine {
    /// The open scene in canonical RON, exactly as `scene.save` would write it.
    pub fn scene_text(&self) -> Result<String, ApiError> {
        let scene = self.scene()?;
        let hints = Hints {
            registry: &self.registry,
            world: Some(&scene.world),
        };
        let doc = snapshot_world(&scene.meta, &scene.world).map_err(|e| map::scene(e, &hints))?;
        write_scene(&doc).map_err(|e| map::scene(e, &hints))
    }
}

/// Refuses to replace the open scene while a transaction is open or (without `discard`) it is dirty.
fn ensure_replaceable(
    engine: &Engine,
    ctx: &CallCtx,
    discard: Option<bool>,
) -> Result<(), ApiError> {
    let Some(scene) = engine.scene.as_ref() else {
        return Ok(());
    };
    if let Some(err) = conflict(scene, ctx) {
        return Err(err);
    }
    match scene.dirty && !discard.unwrap_or(false) {
        true => Err(ApiError::new(
            ApiErrorKind::SceneDirty,
            format!("{} has unsaved changes", scene.path),
        )),
        false => Ok(()),
    }
}

/// Resolves a project-relative `.scene.ron` path, rejecting anything that escapes the project.
fn project_path(engine: &Engine, path: &str) -> Result<PathBuf, ApiError> {
    engine
        .project_file(path, ".scene.ron")
        .map_err(|e| e.suggest(["e.g. \"scenes/main.scene.ron\"".to_owned()]))
}

/// The project folder, for document ops (M4 4.10c).
fn project_documents(engine: &Engine) -> std::sync::Arc<dyn concordia_command::Documents> {
    std::sync::Arc::new(concordia_command::ProjectDocuments::new(engine.project()))
}

fn open_journal(
    engine: &Engine,
    scene: SceneId,
) -> Result<(Journal, concordia_command::Loaded), ApiError> {
    let path = engine
        .project()
        .join(".concordia/journal")
        .join(format!("{scene}.cmdlog.jsonl"));
    Journal::open(&path, engine.config.journal)
        .map_err(|e| ApiError::new(ApiErrorKind::SceneIo, e.to_string()))
}

/// Writes via a temporary file and rename, so a crash never leaves a half-written scene.
fn write_atomically(file: &Path, text: &str) -> std::io::Result<()> {
    concordia_core::fs::write_atomically(
        file,
        text.as_bytes(),
        concordia_core::fs::Visibility::Default,
    )
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<SceneNew>(),
        MethodInfo::of::<SceneOpen>(),
        MethodInfo::of::<SceneSave>(),
        MethodInfo::of::<SceneInfo>(),
        MethodInfo::of::<SceneText>(),
    ]
}
