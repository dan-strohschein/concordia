//! Runs an [`Engine`] on its own thread; transports talk to it through a cloneable async handle.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Instant;

use concordia_core::id::SessionId;
use serde_json::Value;
use tokio::sync::oneshot;

use crate::engine::{Engine, internal};
use crate::error::ApiError;

enum Request {
    Open(oneshot::Sender<SessionId>),
    Close(SessionId),
    SceneText(oneshot::Sender<Result<String, ApiError>>),
    Rpc {
        session: SessionId,
        text: String,
        reply: oneshot::Sender<Option<String>>,
    },
    Call {
        session: SessionId,
        method: String,
        params: Value,
        reply: oneshot::Sender<Result<Value, ApiError>>,
    },
}

/// A cloneable handle to an engine thread. The thread exits when every handle is dropped.
#[derive(Debug, Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<Request>,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Request")
    }
}

impl Engine {
    /// Moves the engine to a dedicated thread (all calls are serialized there).
    pub fn spawn(self) -> EngineHandle {
        self.spawn_with_thread().0
    }

    /// Like [`Engine::spawn`], also returning the thread's join handle.
    pub fn spawn_with_thread(self) -> (EngineHandle, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("concordia-engine".into())
            .spawn(move || run(self, &rx))
            .expect("the OS can start the engine thread");
        (EngineHandle { tx }, thread)
    }
}

fn run(mut engine: Engine, rx: &mpsc::Receiver<Request>) {
    loop {
        let next = match engine.idle_timeout() {
            Some(timeout) => match rx.recv_timeout(timeout) {
                Ok(request) => Some(request),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            },
            None => match rx.recv() {
                Ok(request) => Some(request),
                Err(_) => return,
            },
        };
        engine.poll(Instant::now());
        if let Some(request) = next {
            serve(&mut engine, request);
        }
    }
}

fn serve(engine: &mut Engine, request: Request) {
    match request {
        Request::Open(reply) => {
            let _ = reply.send(engine.open_session());
        }
        Request::Close(session) => engine.close_session(session),
        Request::SceneText(reply) => {
            let _ = reply.send(engine.scene_text());
        }
        Request::Rpc {
            session,
            text,
            reply,
        } => {
            let _ = reply.send(engine.handle_jsonrpc(session, &text));
        }
        Request::Call {
            session,
            method,
            params,
            reply,
        } => {
            let _ = reply.send(engine.call(session, &method, params));
        }
    }
}

impl EngineHandle {
    fn send(&self, request: Request) -> Result<(), ApiError> {
        self.tx
            .send(request)
            .map_err(|_| internal("the engine thread has stopped"))
    }

    /// Starts a session.
    pub async fn open_session(&self) -> Result<SessionId, ApiError> {
        let (reply, rx) = oneshot::channel();
        self.send(Request::Open(reply))?;
        rx.await.map_err(internal)
    }

    /// Ends a session (aborting its open transaction).
    pub fn close_session(&self, session: SessionId) {
        let _ = self.send(Request::Close(session));
    }

    /// The open scene as canonical RON (see [`Engine::scene_text`]).
    pub async fn scene_text(&self) -> Result<String, ApiError> {
        let (reply, rx) = oneshot::channel();
        self.send(Request::SceneText(reply))?;
        rx.await.map_err(internal)?
    }

    /// Handles one JSON-RPC message.
    pub async fn handle_jsonrpc(&self, session: SessionId, text: String) -> Option<String> {
        let (reply, rx) = oneshot::channel();
        self.send(Request::Rpc {
            session,
            text,
            reply,
        })
        .ok()?;
        rx.await.ok().flatten()
    }

    /// Calls a method directly (used by in-process adapters such as MCP).
    pub async fn call(
        &self,
        session: SessionId,
        method: &str,
        params: Value,
    ) -> Result<Value, ApiError> {
        let (reply, rx) = oneshot::channel();
        self.send(Request::Call {
            session,
            method: method.to_owned(),
            params,
            reply,
        })?;
        rx.await.map_err(internal)?
    }
}
