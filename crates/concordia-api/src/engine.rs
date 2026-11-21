//! The engine: registry, open scene (edit world + command log), play world, sessions.

use std::any::Any;
use std::collections::{BTreeSet, HashMap};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use concordia_command::{Actor, CommandLog, JournalConfig};
use concordia_core::id::{DeterministicIdGen, IdError, IdSource, SessionId, SystemIdGen, TxnId};
use concordia_core::jobs::JobSystem;
use concordia_core::log::{LogConfig, LogRing};
use concordia_core::time::TickRate;
use concordia_reflect::TypeRegistry;
use concordia_scene::SceneMeta;
use concordia_terra::{SimLoop, World};
use serde_json::Value;

use crate::error::{ApiError, ApiErrorKind};
use crate::method::{CallCtx, MethodRegistry};
use crate::methods::{WorldSel, world_of};
use crate::module::EngineModule;

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
    /// Modules added after the core module, in order (ADR-0046).
    pub modules: Vec<Arc<dyn EngineModule>>,
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
            modules: Vec::new(),
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
    /// Module runtime state for this play session, dropped with it.
    pub(crate) states: Vec<Box<dyn Any + Send>>,
}

/// Owns all engine state; every call runs on one thread, so mutations are serialized.
pub struct Engine {
    pub(crate) config: EngineConfig,
    pub(crate) registry: TypeRegistry,
    pub(crate) modules: Vec<Arc<dyn EngineModule>>,
    pub(crate) methods: Arc<MethodRegistry>,
    pub(crate) states: Vec<Box<dyn Any + Send>>,
    pub(crate) ids: Ids,
    pub(crate) jobs: JobSystem,
    pub(crate) sessions: HashMap<SessionId, Session>,
    pub(crate) scene: Option<OpenScene>,
    pub(crate) play: Option<Play>,
    pub(crate) ring: Arc<LogRing>,
    pub(crate) events: crate::events::EventHub,
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
    /// Creates an engine with the core module followed by `config.modules`.
    pub fn new(config: EngineConfig) -> Result<Self, ApiError> {
        let modules = crate::module::with_core(&config.modules);
        require_unique_names(&modules)?;
        let registry = register_types(&modules)?;
        let methods = Arc::new(MethodRegistry::build(&modules)?);
        let states = modules.iter().filter_map(|m| m.engine_state()).collect();
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
            modules,
            methods,
            states,
            ids,
            jobs,
            sessions: HashMap::new(),
            scene: None,
            play: None,
            ring,
            events: crate::events::EventHub::default(),
        })
    }

    /// The type registry.
    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    /// Every method this engine serves (core and modules), sorted by name.
    pub fn methods(&self) -> &Arc<MethodRegistry> {
        &self.methods
    }

    /// The edit or play world (`NotPlaying` if there is no play world).
    pub fn world(&self, which: WorldSel) -> Result<&World, ApiError> {
        world_of(self, Some(which))
    }

    /// The play world for writing (e.g. injected input), created paused from the edit world if there is none.
    pub fn play_world_mut(&mut self) -> Result<&mut World, ApiError> {
        crate::methods::sim::ensure_play(self, None).map(|play| &mut play.world)
    }

    /// A module's engine-lifetime state of type `T`, if any.
    pub fn state<T: Any>(&self) -> Option<&T> {
        self.states.iter().find_map(|s| s.downcast_ref::<T>())
    }

    /// A module's engine-lifetime state of type `T`, mutable.
    pub fn state_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.states.iter_mut().find_map(|s| s.downcast_mut::<T>())
    }

    /// The type registry and a module's engine-lifetime state of type `T`, borrowed together.
    pub fn registry_and_state_mut<T: Any>(&mut self) -> (&TypeRegistry, Option<&mut T>) {
        let state = self.states.iter_mut().find_map(|s| s.downcast_mut::<T>());
        (&self.registry, state)
    }

    /// The state of type `T` a module created for the current play session, if any.
    pub fn play_state<T: Any>(&self) -> Option<&T> {
        self.play
            .as_ref()?
            .states
            .iter()
            .find_map(|s| s.downcast_ref::<T>())
    }

    /// The engine's id source (deterministic when configured), for ids a module's edits introduce.
    pub fn ids_mut(&mut self) -> &mut impl IdSource {
        &mut self.ids
    }

    /// Applies `ops` for the caller as one undo step labelled `label`, or inside the caller's
    /// open transaction. On failure nothing is applied (and an open transaction is rolled back).
    pub fn apply_edits(
        &mut self,
        ctx: &CallCtx,
        label: &str,
        ops: Vec<concordia_command::EditOp>,
    ) -> Result<(), ApiError> {
        crate::methods::edit_all(self, ctx, label, ops, None)
    }

    /// Applies `ops` as one transaction attributed to `actor` (not a session), e.g. a script
    /// module's own migration. Refused while any transaction is held; `None` if `ops` is empty.
    pub fn commit_edits_as(
        &mut self,
        actor: concordia_command::Actor,
        label: &str,
        ops: Vec<concordia_command::EditOp>,
    ) -> Result<Option<concordia_core::id::TxnId>, ApiError> {
        crate::methods::commit_as(self, actor, label, ops)
    }

    /// Undoes `txn`, which must be the latest commit, attributed to `actor`.
    pub fn undo_as(
        &mut self,
        actor: concordia_command::Actor,
        txn: concordia_core::id::TxnId,
    ) -> Result<(), ApiError> {
        crate::methods::undo_as(self, actor, txn)
    }

    /// The type registry and the play world (if any), borrowed together.
    pub fn registry_and_play_mut(&mut self) -> (&mut TypeRegistry, Option<&mut World>) {
        (&mut self.registry, self.play.as_mut().map(|p| &mut p.world))
    }

    /// The project's template catalog: module built-ins plus `templates/**/*.template.ron`.
    pub fn template_catalog(&self) -> Result<concordia_terra_sim::TemplateCatalog, ApiError> {
        crate::templates::catalog(self)
    }

    /// The play-world RNG seed (`[sim] seed`).
    pub fn sim_seed(&self) -> u64 {
        self.config.sim_seed
    }

    /// Every module's [`Fingerprint`]s, in module order (2.8).
    pub fn fingerprints(&self) -> Vec<crate::Fingerprint> {
        self.modules.iter().flat_map(|m| m.fingerprints()).collect()
    }

    /// Discards any play world and starts a fresh, paused one from the edit world at tick 0
    /// (at `rate`, or the configured rate).
    pub fn restart_play(
        &mut self,
        rate: Option<concordia_core::time::TickRate>,
    ) -> Result<&mut World, ApiError> {
        self.play = None;
        crate::methods::sim::ensure_play(self, rate).map(|play| &mut play.world)
    }

    /// Advances the play world (created paused if needed) by `ticks` and returns its hash.
    pub fn step_play(&mut self, ticks: u32) -> Result<concordia_terra::WorldHash, ApiError> {
        crate::methods::sim::ensure_play(self, None)?;
        let Engine { play, jobs, .. } = self;
        let play = play
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NotPlaying))?;
        play.sim.step(&mut play.world, jobs, ticks);
        play.world.state_hash().map_err(internal)
    }

    /// The open scene's project-relative path.
    pub fn scene_path(&self) -> Option<&str> {
        self.scene.as_ref().map(|s| s.path.as_str())
    }

    /// Job system threads.
    pub fn threads(&self) -> std::num::NonZeroUsize {
        self.jobs.threads()
    }

    /// The configured tick rate (`[sim] rate_hz`).
    pub fn sim_rate(&self) -> concordia_core::time::TickRate {
        self.config.sim_rate
    }

    /// Resolves a project-relative path ending in `extension`, refusing anything that escapes
    /// the project (`InvalidParams` at `/path`).
    pub fn project_file(&self, path: &str, extension: &str) -> Result<PathBuf, ApiError> {
        let relative = Path::new(path);
        let inside = relative
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !inside || !path.ends_with(extension) || path.len() == extension.len() {
            return Err(ApiError::new(
                ApiErrorKind::InvalidParams,
                format!("{path:?} must be a project-relative path ending in {extension}, without .. or a leading /"),
            )
            .at("/path"));
        }
        Ok(self.config.project.join(relative))
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
        self.events.remove(session);
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
        let before = crate::events::Marker::of(self, method);
        let result = crate::method::dispatch(self, session, method, params);
        if let Ok(value) = &result {
            crate::events::publish_after(self, method, &before, value);
        }
        result
    }

    /// Sends an `events.event` on `topic` to subscribed sessions (module methods use it, e.g. Studio).
    pub fn publish_event(&mut self, topic: crate::events::Topic, params: serde_json::Value) {
        if self.events.wants(topic) {
            self.events.publish(topic, params);
        }
    }

    /// Lets `session` receive pushed events (transports that can push call this once per session).
    pub fn set_event_sink(&mut self, session: SessionId, sink: crate::events::EventSink) {
        self.events.set_sink(session, sink);
    }

    /// Housekeeping: expires transactions and advances a playing simulation.
    pub fn poll(&mut self, now: Instant) {
        let timeout = self.config.txn_timeout;
        self.abort_held(|hold| now.duration_since(hold.since) >= timeout, "timeout");
        let mut advanced = false;
        if let Some(play) = self.play.as_mut().filter(|p| p.playing) {
            let real = now.saturating_duration_since(play.last);
            play.last = now;
            advanced = play.sim.advance(&mut play.world, &self.jobs, real).steps > 0;
        }
        if advanced {
            for module in self.modules.clone() {
                module.after_sim(self);
            }
        }
    }

    /// How long the engine thread may sleep before `poll` has work.
    pub(crate) fn idle_timeout(&self) -> Option<Duration> {
        if let Some(play) = self.play.as_ref().filter(|p| p.playing) {
            // Wake exactly when the next tick is due: one tick per wake, no judder.
            return Some(
                play.sim
                    .until_next_step()
                    .saturating_sub(play.last.elapsed()),
            );
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
            if let Err(e) = scene
                .log
                .abort(&mut scene.world, &mut self.registry, reason)
            {
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

fn require_unique_names(modules: &[Arc<dyn EngineModule>]) -> Result<(), ApiError> {
    let mut seen = BTreeSet::new();
    match modules.iter().find(|m| !seen.insert(m.name())) {
        Some(m) => Err(internal(format!("module {} is added twice", m.name()))),
        None => Ok(()),
    }
}

fn register_types(modules: &[Arc<dyn EngineModule>]) -> Result<TypeRegistry, ApiError> {
    let mut registry = TypeRegistry::new();
    for module in modules {
        module
            .register_types(&mut registry)
            .map_err(|e| internal(format!("module {}: {e}", module.name())))?;
    }
    Ok(registry)
}

/// Wraps an unexpected failure.
pub(crate) fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ApiErrorKind::Internal, e.to_string())
}
