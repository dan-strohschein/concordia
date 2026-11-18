//! JSON-RPC over WebSocket: localhost only, with a startup token and an Origin check.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

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

/// A running WebSocket server (stops when dropped).
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

/// Starts serving; each connection is one session.
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
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(connection(handle.clone(), stream, token.clone()));
        }
    });
    Ok(WsServer {
        local_addr,
        token: config.token,
        task,
    })
}

async fn connection(handle: EngineHandle, stream: TcpStream, token: String) {
    let check = Handshake { token: &token };
    let Ok(mut socket) = tokio_tungstenite::accept_hdr_async(stream, check).await else {
        return;
    };
    let Ok(session) = handle.open_session().await else {
        return;
    };
    while let Some(Ok(message)) = socket.next().await {
        match message {
            Message::Text(text) => {
                if let Some(reply) = handle.handle_jsonrpc(session, text.to_string()).await
                    && socket.send(Message::text(reply)).await.is_err()
                {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
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

/// Writes `<project>/.concordia/headless.json` (`url`, `token`, `pid`), readable only by the owner.
pub fn write_connection_file(project: &Path, server: &WsServer) -> std::io::Result<PathBuf> {
    let dir = project.join(".concordia");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("headless.json");
    let body = serde_json::json!({ "url": server.url(), "token": server.token, "pid": std::process::id() });
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&path)?;
    std::io::Write::write_all(&mut file, body.to_string().as_bytes())?;
    Ok(path)
}
