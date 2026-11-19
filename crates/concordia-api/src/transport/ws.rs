//! JSON-RPC over WebSocket: localhost only, with a startup token and an Origin check.

use std::net::SocketAddr;

use futures::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::http::StatusCode;

use crate::engine::internal;
use crate::error::{ApiError, ApiErrorKind};
use crate::handle::EngineHandle;

/// WebSocket settings.
#[derive(Debug, Clone)]
pub struct WsConfig {
    /// Bind address; must be a loopback address.
    pub addr: SocketAddr,
    /// Clients pass it as `?token=...`.
    pub token: String,
}

impl WsConfig {
    /// `127.0.0.1:port` (0 picks a free port) with a fresh random token.
    pub fn localhost(port: u16) -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], port)),
            token: uuid::Uuid::new_v4().simple().to_string(),
        }
    }
}

/// A running WebSocket server; dropping it stops it and closes every connection.
#[derive(Debug)]
pub struct WsServer {
    /// The bound address.
    pub local_addr: SocketAddr,
    /// The access token.
    pub token: String,
    task: tokio::task::JoinHandle<()>,
}

impl WsServer {
    /// `ws://127.0.0.1:<port>/` (append `?token=...`).
    pub fn url(&self) -> String {
        format!("ws://{}/", self.local_addr)
    }
}

impl Drop for WsServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Pushed events buffered per connection; newer events are dropped while it is full.
const EVENT_QUEUE: usize = 1024;

/// Starts serving; each connection is one session (with an event sink for `events.subscribe`).
pub async fn serve(handle: EngineHandle, config: WsConfig) -> Result<WsServer, ApiError> {
    if !config.addr.ip().is_loopback() {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("{} is not a loopback address", config.addr),
        )
        .suggest(["the WebSocket transport binds to 127.0.0.1 or ::1 only".to_owned()]));
    }
    let listener = TcpListener::bind(config.addr).await.map_err(internal)?;
    let local_addr = listener.local_addr().map_err(internal)?;
    let token = config.token.clone();
    let task = tokio::spawn(accept(listener, handle, token));
    Ok(WsServer {
        local_addr,
        token: config.token,
        task,
    })
}

/// Accepts connections; they live in a set owned by this task, so stopping it closes them all.
async fn accept(listener: TcpListener, handle: EngineHandle, token: String) {
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    connections.spawn(connection(handle.clone(), stream, token.clone()));
                }
                // Transient (e.g. out of file descriptors): back off rather than drop live clients.
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}

async fn connection(handle: EngineHandle, stream: TcpStream, token: String) {
    let check = Handshake { token: &token };
    let Ok(mut socket) = tokio_tungstenite::accept_hdr_async(stream, check).await else {
        return;
    };
    let Ok(session) = handle.open_session().await else {
        return;
    };
    // Events queue here; while EVENT_QUEUE are unsent, newer ones are dropped (a slow client
    // should re-read state with query.run).
    let (events_tx, mut events) = tokio::sync::mpsc::channel::<String>(EVENT_QUEUE);
    handle.set_event_sink(
        session,
        Box::new(move |event| {
            let _ = events_tx.try_send(event.to_string());
        }),
    );
    loop {
        tokio::select! {
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Some(reply) = handle.handle_jsonrpc(session, text.to_string()).await
                        && socket.send(Message::text(reply)).await.is_err()
                    {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            Some(event) = events.recv() => {
                if socket.send(Message::text(event)).await.is_err() {
                    break;
                }
            }
        }
    }
    handle.close_session(session);
}

/// Token and Origin check run during the WebSocket handshake.
struct Handshake<'a> {
    token: &'a str,
}

#[allow(clippy::result_large_err)] // the `ErrorResponse` type is fixed by tungstenite's `Callback` trait
impl Callback for Handshake<'_> {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        match authorize(request, self.token) {
            Ok(()) => Ok(response),
            Err((status, message)) => {
                let mut rejection = ErrorResponse::new(Some(message.to_owned()));
                *rejection.status_mut() = status;
                Err(rejection)
            }
        }
    }
}

/// 401 without the right token; 403 for a non-local browser Origin.
fn authorize(request: &Request, token: &str) -> Result<(), (StatusCode, &'static str)> {
    let given = request
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("token=")));
    if given != Some(token) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "missing or wrong token; connect with ?token=<token>",
        ));
    }
    match request
        .headers()
        .get("origin")
        .map(|o| o.to_str().unwrap_or(""))
    {
        Some(origin) if !local_origin(origin) => {
            Err((StatusCode::FORBIDDEN, "only localhost origins may connect"))
        }
        _ => Ok(()),
    }
}

fn local_origin(origin: &str) -> bool {
    let Some((scheme, rest)) = origin.split_once("://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    let host = match host.rsplit_once(':') {
        Some((h, port)) if port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => host,
    };
    matches!(scheme, "http" | "https" | "tauri")
        && matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}
