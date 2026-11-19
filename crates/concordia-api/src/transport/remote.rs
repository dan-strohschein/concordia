//! [`RemoteEngine`]: an [`EngineLink`] to a live engine in another process, over WebSocket.
//!
//! Each engine session is its own WebSocket connection (the server opens one session per
//! connection), so per-agent sessions stay separate on the host. When the host exits, pending
//! and later calls fail with `EngineExited`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use concordia_core::id::{IdSource, SessionId, SystemIdGen};
use futures::future::BoxFuture;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use super::connection::ConnectionInfo;
use crate::engine::internal;
use crate::error::{ApiError, ApiErrorKind};
use crate::link::EngineLink;
use crate::method::MethodRegistry;

type Reply = oneshot::Sender<Result<Value, ApiError>>;
type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A live engine in another process.
#[derive(Debug, Clone)]
pub struct RemoteEngine {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    info: ConnectionInfo,
    methods: Arc<MethodRegistry>,
    exited: Arc<AtomicBool>,
    sessions: Mutex<HashMap<SessionId, Connection>>,
    /// Held open so an idle client still notices the host's exit.
    _watch: Connection,
}

/// One WebSocket connection (one host session); its task ends when this is dropped.
#[derive(Debug, Clone)]
struct Connection {
    requests: mpsc::UnboundedSender<(String, Value, Reply)>,
}

impl RemoteEngine {
    /// Connects to the engine `info` describes; `methods` is the set it serves (built from the
    /// same project). Fails if the server is gone or refuses the token.
    pub async fn connect(
        info: &ConnectionInfo,
        methods: Arc<MethodRegistry>,
    ) -> Result<Self, ApiError> {
        let exited = Arc::new(AtomicBool::new(false));
        let watch = open(info, &exited).await?;
        Ok(Self {
            inner: Arc::new(Inner {
                info: info.clone(),
                methods,
                exited,
                sessions: Mutex::new(HashMap::new()),
                _watch: watch,
            }),
        })
    }

    /// The connection file this engine was found through.
    pub fn info(&self) -> &ConnectionInfo {
        &self.inner.info
    }

    /// True once the host has gone away.
    pub fn exited(&self) -> bool {
        self.inner.exited.load(Ordering::SeqCst)
    }

    fn gone(&self) -> ApiError {
        exited_error(&self.inner.info)
    }

    fn connection(&self, session: SessionId) -> Result<Connection, ApiError> {
        let sessions = self
            .inner
            .sessions
            .lock()
            .map_err(|_| internal("the remote session table is poisoned"))?;
        sessions.get(&session).cloned().ok_or_else(|| {
            ApiError::new(
                ApiErrorKind::InvalidRequest,
                format!("session {session} is not open on this client"),
            )
        })
    }

    async fn request(
        &self,
        session: SessionId,
        method: &str,
        params: Value,
    ) -> Result<Value, ApiError> {
        if self.exited() {
            return Err(self.gone());
        }
        let connection = self.connection(session)?;
        let (reply, answer) = oneshot::channel();
        connection
            .requests
            .send((method.to_owned(), params, reply))
            .map_err(|_| self.gone())?;
        answer.await.unwrap_or_else(|_| Err(self.gone()))
    }

    async fn new_session(&self) -> Result<SessionId, ApiError> {
        if self.exited() {
            return Err(self.gone());
        }
        let connection = open(&self.inner.info, &self.inner.exited).await?;
        let session = SystemIdGen.next_id().map_err(internal)?;
        self.inner
            .sessions
            .lock()
            .map_err(|_| internal("the remote session table is poisoned"))?
            .insert(session, connection);
        Ok(session)
    }
}

impl EngineLink for RemoteEngine {
    fn methods(&self) -> &MethodRegistry {
        &self.inner.methods
    }

    fn open_session(&self) -> BoxFuture<'_, Result<SessionId, ApiError>> {
        Box::pin(self.new_session())
    }

    fn close_session(&self, session: SessionId) {
        if let Ok(mut sessions) = self.inner.sessions.lock() {
            sessions.remove(&session);
        }
    }

    fn call<'a>(
        &'a self,
        session: SessionId,
        method: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, ApiError>> {
        Box::pin(self.request(session, method, params))
    }
}

/// `EngineExited`, naming the host.
fn exited_error(info: &ConnectionInfo) -> ApiError {
    ApiError::new(
        ApiErrorKind::EngineExited,
        format!(
            "the shared engine at {} (pid {}, hosted by {}) has exited",
            info.url, info.pid, info.host
        ),
    )
    .with("pid", json!(info.pid))
}

/// Opens one WebSocket connection and starts its task.
async fn open(info: &ConnectionInfo, exited: &Arc<AtomicBool>) -> Result<Connection, ApiError> {
    let socket = match tokio_tungstenite::connect_async(info.connect_url()).await {
        Ok((socket, _)) => socket,
        Err(WsError::Http(response)) => {
            return Err(unreachable(
                info,
                &format!(
                    "refused the token (HTTP {}); the connection file is stale",
                    response.status().as_u16()
                ),
            ));
        }
        Err(e) => return Err(unreachable(info, &format!("does not answer ({e})"))),
    };
    let (requests, queue) = mpsc::unbounded_channel();
    tokio::spawn(run(socket, queue, info.clone(), Arc::clone(exited)));
    Ok(Connection { requests })
}

fn unreachable(info: &ConnectionInfo, why: &str) -> ApiError {
    ApiError::new(
        ApiErrorKind::EngineExited,
        format!("the engine at {} (pid {}) {why}", info.url, info.pid),
    )
}

/// Sends queued requests and routes replies by id until the queue closes or the host goes away.
async fn run(
    mut socket: Socket,
    mut queue: mpsc::UnboundedReceiver<(String, Value, Reply)>,
    info: ConnectionInfo,
    exited: Arc<AtomicBool>,
) {
    let mut pending: HashMap<u64, Reply> = HashMap::new();
    let mut next_id = 0_u64;
    loop {
        tokio::select! {
            request = queue.recv() => {
                let Some((method, params, reply)) = request else {
                    let _ = socket.close(None).await;
                    return;
                };
                next_id += 1;
                let text = json!({"jsonrpc": "2.0", "id": next_id, "method": method, "params": params});
                pending.insert(next_id, reply);
                if socket.send(Message::text(text.to_string())).await.is_err() {
                    break;
                }
            }
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => route(&text, &mut pending),
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
    exited.store(true, Ordering::SeqCst);
    let gone = exited_error(&info);
    for (_, reply) in pending.drain() {
        let _ = reply.send(Err(gone.clone()));
    }
    while let Some((_, _, reply)) = queue.recv().await {
        let _ = reply.send(Err(gone.clone()));
    }
}

/// Answers the pending request a response belongs to; notifications are ignored.
fn route(text: &str, pending: &mut HashMap<u64, Reply>) {
    let Ok(response) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let Some(reply) = response["id"].as_u64().and_then(|id| pending.remove(&id)) else {
        return;
    };
    let result = match response.get("error") {
        Some(error) => Err(ApiError::from_jsonrpc(error)),
        None => Ok(response.get("result").cloned().unwrap_or(Value::Null)),
    };
    let _ = reply.send(result);
}
