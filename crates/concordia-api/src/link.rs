//! [`EngineLink`]: how a client reaches an engine, in-process or over WebSocket (ADR-0082).

use concordia_core::id::SessionId;
use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::ApiError;
use crate::handle::EngineHandle;
use crate::method::MethodRegistry;

/// Sessions and calls on an engine, wherever it runs.
pub trait EngineLink: Send + Sync + std::fmt::Debug {
    /// Every method the engine serves.
    fn methods(&self) -> &MethodRegistry;

    /// Starts a session.
    fn open_session(&self) -> BoxFuture<'_, Result<SessionId, ApiError>>;

    /// Ends a session (aborting its open transaction).
    fn close_session(&self, session: SessionId);

    /// Calls `method` in `session`.
    fn call<'a>(
        &'a self,
        session: SessionId,
        method: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, ApiError>>;
}

impl EngineLink for EngineHandle {
    fn methods(&self) -> &MethodRegistry {
        EngineHandle::methods(self)
    }

    fn open_session(&self) -> BoxFuture<'_, Result<SessionId, ApiError>> {
        Box::pin(EngineHandle::open_session(self))
    }

    fn close_session(&self, session: SessionId) {
        EngineHandle::close_session(self, session);
    }

    fn call<'a>(
        &'a self,
        session: SessionId,
        method: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, ApiError>> {
        Box::pin(EngineHandle::call(self, session, method, params))
    }
}
