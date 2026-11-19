//! `.concordia/headless.json`: how clients find a project's live engine (ADR-0082).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ws::WsServer;

/// Which kind of process hosts the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    /// The editor (`concordia-editor`).
    Editor,
    /// `concordia mcp`, started by Claude Code.
    Mcp,
    /// `concordia headless --ws`.
    Headless,
}

impl std::fmt::Display for HostKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Editor => "the editor",
            Self::Mcp => "concordia mcp",
            Self::Headless => "concordia headless",
        })
    }
}

/// The contents of `.concordia/headless.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// `ws://127.0.0.1:<port>/`.
    pub url: String,
    /// The access token (`?token=`).
    pub token: String,
    /// The hosting process.
    pub pid: u32,
    /// The host's engine version.
    pub version: String,
    /// The kind of host.
    pub host: HostKind,
}

impl ConnectionInfo {
    /// The file's path in `project`.
    pub fn path(project: &Path) -> PathBuf {
        project.join(".concordia").join("headless.json")
    }

    /// The details of `server`, hosted by this process.
    pub fn for_server(server: &WsServer, host: HostKind) -> Self {
        Self {
            url: server.url(),
            token: server.token.clone(),
            pid: std::process::id(),
            version: concordia_core::version().to_owned(),
            host,
        }
    }

    /// The URL with the token, ready to connect.
    pub fn connect_url(&self) -> String {
        format!("{}?token={}", self.url, self.token)
    }

    /// Reads the file; `None` if it is missing or unreadable as a connection file.
    pub fn read(project: &Path) -> std::io::Result<Option<Self>> {
        match std::fs::read_to_string(Self::path(project)) {
            Ok(text) => Ok(serde_json::from_str(&text).ok()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Writes the file atomically, readable only by the owner.
    pub fn write(&self, project: &Path) -> std::io::Result<PathBuf> {
        let path = Self::path(project);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        concordia_core::fs::write_atomically(
            &path,
            &body,
            concordia_core::fs::Visibility::OwnerOnly,
        )?;
        Ok(path)
    }

    /// Deletes the file if it still describes this engine (same token); true if it did.
    pub fn remove_if_current(&self, project: &Path) -> std::io::Result<bool> {
        match Self::read(project)? {
            Some(current) if current.token == self.token => {
                match std::fs::remove_file(Self::path(project)) {
                    Ok(()) => Ok(true),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                    Err(e) => Err(e),
                }
            }
            _ => Ok(false),
        }
    }
}
