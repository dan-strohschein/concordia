//! The Studio engine module (ADR-0046): `task.*` and `studio.*`, enabled by `[studio]` in `concordia.toml`.

use std::path::{Path, PathBuf};

use concordia_api::{ApiError, ApiErrorKind, Engine, EngineModule, MethodInfo, Topic};

use crate::error::StudioError;
use crate::methods;
use crate::roster::Roster;
use crate::scope::ToolCatalog;
use crate::task::{BoardError, TaskBoard, TaskStore};

/// Adds the task board and the `task.*` / `studio.*` methods for a project.
#[derive(Debug, Clone)]
pub struct StudioModule {
    root: PathBuf,
    dir: PathBuf,
}

impl StudioModule {
    /// The module for the project at `root`, with Studio files in `<root>/<dir>`.
    pub fn new(root: &Path, dir: impl Into<PathBuf>) -> Self {
        Self {
            root: root.to_owned(),
            dir: dir.into(),
        }
    }
}

impl EngineModule for StudioModule {
    fn name(&self) -> &'static str {
        "studio"
    }

    fn methods(&self) -> Vec<MethodInfo> {
        methods::all()
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(StudioState {
            studio: self.root.join(&self.dir),
            dir: self.dir.clone(),
            board: TaskBoard::new(TaskStore::new(&self.root, &self.dir)),
        }))
    }
}

/// The module's engine-lifetime state.
#[derive(Debug)]
pub(crate) struct StudioState {
    pub(crate) studio: PathBuf,
    pub(crate) dir: PathBuf,
    pub(crate) board: TaskBoard,
}

pub(crate) fn state(engine: &mut Engine) -> Result<&mut StudioState, ApiError> {
    engine
        .state_mut::<StudioState>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the Studio module is not loaded"))
}

/// The roster, re-read from `studio/` on every call so edits apply at once.
pub(crate) fn roster(engine: &mut Engine) -> Result<Roster, ApiError> {
    let catalog = ToolCatalog::from_methods(engine.methods());
    let state = state(engine)?;
    let (studio, dir) = (state.studio.clone(), state.dir.clone());
    if !studio.join("team.toml").is_file() {
        return Err(ApiError::new(
            ApiErrorKind::SceneIo,
            format!("this project has no {}/team.toml", dir.display()),
        )
        .suggest(["run `concordia studio init` to add Agent Studio to the project".to_owned()]));
    }
    Roster::load(&studio, &catalog).map_err(config_error)
}

/// Publishes journal entries written since the last call as `studio` events.
pub(crate) fn publish(engine: &mut Engine) -> Result<(), ApiError> {
    let entries = state(engine)?
        .board
        .take_new_entries()
        .map_err(board_error)?;
    for entry in entries {
        engine.publish_event(Topic::studio, entry);
    }
    Ok(())
}

/// Maps a config error to the API.
pub(crate) fn config_error(e: StudioError) -> ApiError {
    let kind = match e {
        StudioError::Io { .. } | StudioError::Journal { .. } => ApiErrorKind::SceneIo,
        _ => ApiErrorKind::SceneParse,
    };
    ApiError::new(kind, e.to_string())
        .suggest(["fix the Studio file; `concordia studio roster` checks it".to_owned()])
}

/// Maps a board error to the API.
pub(crate) fn board_error(e: BoardError) -> ApiError {
    match e {
        BoardError::NotFound(id) => {
            ApiError::new(ApiErrorKind::TaskNotFound, format!("no task {id}"))
        }
        BoardError::Transition {
            message,
            suggestions,
        } => ApiError::new(ApiErrorKind::TaskTransition, message).suggest(suggestions),
        BoardError::NotPermitted(message) => ApiError::new(ApiErrorKind::NotPermitted, message),
        BoardError::Invalid {
            message,
            suggestions,
        } => ApiError::new(ApiErrorKind::InvalidParams, message).suggest(suggestions),
        BoardError::Studio(e) => config_error(e),
    }
}
