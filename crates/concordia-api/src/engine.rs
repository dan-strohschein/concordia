//! The engine: registry, open scene (edit world + command log), play world, sessions.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use concordia_command::{Actor, CommandLog, JournalConfig};
use concordia_core::id::{DeterministicIdGen, IdError, IdSource, SessionId, SystemIdGen, TxnId};
use concordia_core::jobs::JobSystem;
use concordia_core::log::{LogConfig, LogRing};
use concordia_core::time::TickRate;
use concordia_ecs::{SimLoop, World};
use concordia_reflect::TypeRegistry;
use concordia_scene::SceneMeta;
use serde_json::Value;

use crate::error::{ApiError, ApiErrorKind};

/// Engine settings.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Project root; scene paths are relative to it.
    pub project: PathBuf,
    /// Job system threads.
    pub threads: NonZeroUsize,
    /// Seed for reproducible ids (tests and gates); `None` uses the clock.
    pub deterministic_ids: Option<u64>,
    /// Open transactions are aborted after this long.
    pub txn_timeout: Duration,
    /// Journal options.
    pub journal: JournalConfig,
    /// Default simulation rate.
    pub sim_rate: TickRate,
    /// Seed for the play world's `SimRng`.
    pub sim_seed: u64,
    /// Log ring served by `log.tail` (a private ring if `None`).
    pub log_ring: Option<Arc<LogRing>>,
}

impl EngineConfig {
    /// Defaults for a project at `project`.
    pub fn new(project: impl AsRef<Path>) -> Self {
        Self {
            project: project.as_ref().to_owned(),
            threads: std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN),
            deterministic_ids: None,
            txn_timeout: Duration::from_secs(120),
            journal: JournalConfig::default(),
            sim_rate: TickRate::HZ_60,
            sim_seed: 0,
            log_ring: None,
        }
    }
}

/// Id source chosen by configuration.
pub(crate) enum Ids {
    System(SystemIdGen),
    Deterministic(DeterministicIdGen),
}

impl IdSource for Ids {
    fn next_uuid(&mut self) -> Result<concordia_core::id::Uuid, IdError> {
        match self {
            Self::System(ids) => ids.next_uuid(),
            Self::Deterministic(ids) => ids.next_uuid(),
        }
    }
}

/// Per-session state.
#[derive(Debug, Clone, Default)]
pub(crate) struct Session {
    pub(crate) client: Option<String>,
    pub(crate) actor: Option<Actor>,
}

/// Which session holds the open transaction, and since when.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TxnHold {
    pub(crate) session: SessionId,
    pub(crate) txn: TxnId,
    pub(crate) since: Instant,
}

/// The open scene: its edit world and history.
pub(crate) struct OpenScene {
    pub(crate) meta: SceneMeta,
    pub(crate) path: String,
    pub(crate) world: World,
    pub(crate) log: CommandLog,
    pub(crate) dirty: bool,
    pub(crate) hold: Option<TxnHold>,
}

/// The play world (a clone of the edit world; simulation never touches the edit world).
pub(crate) struct Play {
    pub(crate) world: World,
    pub(crate) sim: SimLoop,
    pub(crate) playing: bool,
    pub(crate) last: Instant,
}

/// Owns all engine state; every call runs on one thread, so mutations are serialized.
pub struct Engine {
    pub(crate) config: EngineConfig,
    pub(crate) registry: TypeRegistry,
    pub(crate) ids: Ids,
    pub(crate) jobs: JobSystem,
    pub(crate) sessions: HashMap<SessionId, Session>,
    pub(crate) scene: Option<OpenScene>,
    pub(crate) play: Option<Play>,
    pub(crate) ring: Arc<LogRing>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("project", &self.config.project)
            .field("sessions", &self.sessions.len())
            .field("scene", &self.scene.as_ref().map(|s| &s.path))
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Creates an engine with the built-in components registered.
    pub fn new(config: EngineConfig) -> Result<Self, ApiError> {
        let mut registry = TypeRegistry::new();
        concordia_scene::builtin::register(&mut registry).map_err(internal)?;
        let jobs = JobSystem::new(config.threads).map_err(internal)?;
        let ids = match config.deterministic_ids {
            Some(seed) => Ids::Deterministic(DeterministicIdGen::new(seed)),
            None => Ids::System(SystemIdGen),
        };
        let ring = config
            .log_ring
            .clone()
            .unwrap_or_else(|| Arc::new(LogRing::new(LogConfig::DEFAULT_RING_CAPACITY)));
        Ok(Self {
            config,
            registry,
            ids,
            jobs,
            sessions: HashMap::new(),
            scene: None,
            play: None,
            ring,
        })
    }

    /// The type registry.
    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    /// The project root.
    pub fn project(&self) -> &Path {
        &self.config.project
    }

    /// Starts a session (one per connection).
    pub fn open_session(&mut self) -> SessionId {
        let id = self
            .ids
            .next_id()
            .unwrap_or_else(|_| SystemIdGen.next_id().expect("system ids never fail"));
        self.sessions.insert(id, Session::default());
        id
    }

    /// Ends a session, aborting any transaction it holds.
    pub fn close_session(&mut self, session: SessionId) {
        self.sessions.remove(&session);
        self.abort_held(|hold| hold.session == session, "session_closed");
    }

    /// Calls `method` for `session`.
    pub fn call(
        &mut self,
        session: SessionId,
        method: &str,
        params: Value,
    ) -> Result<Value, ApiError> {
        self.poll(Instant::now());
        crate::method::dispatch(self, session, method, params)
    }

    /// Housekeeping: expires transactions and advances a playing simulation.
    pub fn poll(&mut self, now: Instant) {
        let timeout = self.config.txn_timeout;
        self.abort_held(|hold| now.duration_since(hold.since) >= timeout, "timeout");
        if let Some(play) = self.play.as_mut().filter(|p| p.playing) {
            let real = now.saturating_duration_since(play.last);
            play.last = now;
            play.sim.advance(&mut play.world, &self.jobs, real);
        }
    }

    /// How long the engine thread may sleep before `poll` has work.
    pub(crate) fn idle_timeout(&self) -> Option<Duration> {
        if self.play.as_ref().is_some_and(|p| p.playing) {
            return Some(Duration::from_millis(16));
        }
        let hold = self.scene.as_ref()?.hold?;
        Some(self.config.txn_timeout.saturating_sub(hold.since.elapsed()))
    }

    fn abort_held(&mut self, matches: impl Fn(&TxnHold) -> bool, reason: &str) {
        let Some(scene) = self.scene.as_mut() else {
            return;
        };
        if scene.hold.as_ref().is_some_and(&matches) {
            scene.hold = None;
            if let Err(e) = scene.log.abort(&mut scene.world, &self.registry, reason) {
                tracing::error!(error = %e, reason, "failed to abort transaction");
            }
        }
    }

    pub(crate) fn scene(&self) -> Result<&OpenScene, ApiError> {
        self.scene
            .as_ref()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))
    }

    pub(crate) fn scene_mut(&mut self) -> Result<&mut OpenScene, ApiError> {
        self.scene
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))
    }
}

/// Wraps an unexpected failure.
pub(crate) fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ApiErrorKind::Internal, e.to_string())
}
