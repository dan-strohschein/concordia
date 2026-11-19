//! One engine per project (ADR-0082): attach to the live engine `.concordia/headless.json`
//! names, or host one and publish that file.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use concordia_api::transport::connection::{ConnectionInfo, HostKind};
use concordia_api::transport::remote::RemoteEngine;
use concordia_api::transport::ws::{self, WsServer};
use concordia_api::{Engine, EngineConfig, EngineHandle, EngineLink, EngineModule, MethodRegistry};

use crate::error::AppError;

/// How long a start waits for another process that is starting the engine.
const START_WAIT: Duration = Duration::from_secs(10);

/// This process's way into the project's engine.
#[derive(Debug)]
pub enum SharedEngine {
    /// The engine runs here and is published for others.
    Hosted(HostedEngine),
    /// The engine runs in another process.
    Attached(RemoteEngine),
}

/// An engine hosted by this process; dropping it stops the server and removes its connection file.
#[derive(Debug)]
pub struct HostedEngine {
    handle: EngineHandle,
    server: WsServer,
    info: ConnectionInfo,
    root: PathBuf,
}

impl SharedEngine {
    /// Attaches to the project's live engine, or hosts one from `config` and publishes it as `host`.
    /// A stale connection file (dead pid, refused token) is replaced.
    pub async fn attach_or_host(config: EngineConfig, host: HostKind) -> Result<Self, AppError> {
        Self::attach_or_host_on(config, host, 0).await
    }

    /// [`SharedEngine::attach_or_host`], hosting on localhost `port` (0 picks a free one).
    pub async fn attach_or_host_on(
        config: EngineConfig,
        host: HostKind,
        port: u16,
    ) -> Result<Self, AppError> {
        let _lock = start_lock(&config.project).await?;
        let methods = Arc::new(MethodRegistry::with_core(&config.modules)?);
        match live(&config.project, methods).await? {
            Some(remote) => Ok(Self::Attached(remote)),
            None => Ok(Self::Hosted(HostedEngine::start(config, host, port).await?)),
        }
    }

    /// The project's live engine, if one answers; `modules` are the project's (to list its methods).
    pub async fn attach_live(
        project: &Path,
        modules: &[Arc<dyn EngineModule>],
    ) -> Result<Option<RemoteEngine>, AppError> {
        live(project, Arc::new(MethodRegistry::with_core(modules)?)).await
    }

    /// Sessions and calls on the engine, wherever it runs.
    pub fn link(&self) -> &dyn EngineLink {
        match self {
            Self::Hosted(hosted) => &hosted.handle,
            Self::Attached(remote) => remote,
        }
    }

    /// The in-process engine, when this process hosts it.
    pub fn handle(&self) -> Option<&EngineHandle> {
        match self {
            Self::Hosted(hosted) => Some(&hosted.handle),
            Self::Attached(_) => None,
        }
    }

    /// The published connection details.
    pub fn info(&self) -> &ConnectionInfo {
        match self {
            Self::Hosted(hosted) => &hosted.info,
            Self::Attached(remote) => remote.info(),
        }
    }

    /// True when the engine runs in this process.
    pub fn is_host(&self) -> bool {
        matches!(self, Self::Hosted(_))
    }
}

impl HostedEngine {
    /// Starts the engine and its WebSocket server, then publishes the connection file.
    async fn start(config: EngineConfig, host: HostKind, port: u16) -> Result<Self, AppError> {
        let root = config.project.clone();
        let handle = Engine::new(config)?.spawn();
        let server = ws::serve(handle.clone(), ws::WsConfig::localhost(port)).await?;
        let info = ConnectionInfo::for_server(&server, host);
        info.write(&root)
            .map_err(|e| file_error(&ConnectionInfo::path(&root), &e))?;
        Ok(Self {
            handle,
            server,
            info,
            root,
        })
    }

    /// The WebSocket server clients attach through.
    pub fn server(&self) -> &WsServer {
        &self.server
    }
}

impl Drop for HostedEngine {
    fn drop(&mut self) {
        let _ = self.info.remove_if_current(&self.root);
    }
}

/// The engine the project's connection file names, if it is alive, accepts its token and runs
/// this version; `None` means there is none or the file is stale.
async fn live(
    project: &Path,
    methods: Arc<MethodRegistry>,
) -> Result<Option<RemoteEngine>, AppError> {
    let path = ConnectionInfo::path(project);
    let Some(info) = ConnectionInfo::read(project).map_err(|e| file_error(&path, &e))? else {
        return Ok(None);
    };
    if !concordia_core::process::alive(info.pid) {
        return Ok(None);
    }
    let Ok(remote) = RemoteEngine::connect(&info, methods).await else {
        return Ok(None);
    };
    let ours = concordia_core::version();
    if info.version != ours {
        return Err(AppError::VersionMismatch {
            pid: info.pid,
            host: info.host,
            theirs: info.version,
            ours: ours.to_owned(),
        });
    }
    Ok(Some(remote))
}

/// Serializes attach-or-host decisions per project (`.concordia/engine.lock`, released on drop).
async fn start_lock(project: &Path) -> Result<File, AppError> {
    let path = project.join(".concordia").join("engine.lock");
    let error = |e: &std::io::Error| file_error(&path, e);
    std::fs::create_dir_all(project.join(".concordia")).map_err(|e| error(&e))?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| error(&e))?;
    let deadline = Instant::now() + START_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(TryLockError::WouldBlock) => {
                return Err(AppError::File {
                    path,
                    message: format!(
                        "another process has been starting this project's engine for over {}s",
                        START_WAIT.as_secs()
                    ),
                });
            }
            Err(TryLockError::Error(e)) => return Err(error(&e)),
        }
    }
}

fn file_error(path: &Path, e: &std::io::Error) -> AppError {
    AppError::File {
        path: path.to_owned(),
        message: e.to_string(),
    }
}
