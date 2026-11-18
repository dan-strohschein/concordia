//! `session.open`.

use concordia_command::Actor;
use concordia_core::id::SessionId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::error::ApiError;
use crate::method::{CallCtx, Method, MethodInfo};

/// Engine API protocol version.
pub const PROTOCOL_VERSION: u32 = 1;

/// Identify the client and set this session's default actor, used when a mutating call omits "actor".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SessionOpen;

/// `session.open` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SessionOpenParams {
    /// Client name, e.g. "claude-code" or "editor".
    pub client: String,
    /// Default actor for mutating calls in this session.
    pub actor: Option<Actor>,
}

/// `session.open` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SessionOpenResult {
    /// This session's id.
    pub session: SessionId,
    /// Engine version.
    pub concordia_version: String,
    /// Engine API protocol version.
    pub protocol: u32,
}

impl Method for SessionOpen {
    const NAME: &'static str = "session.open";
    const MUTATES: bool = false;
    type Params = SessionOpenParams;
    type Result = SessionOpenResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: SessionOpenParams,
    ) -> Result<SessionOpenResult, ApiError> {
        let session = engine.sessions.entry(ctx.session).or_default();
        session.client = Some(p.client);
        session.actor = p.actor;
        Ok(SessionOpenResult {
            session: ctx.session,
            concordia_version: concordia_core::version().to_owned(),
            protocol: PROTOCOL_VERSION,
        })
    }
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![MethodInfo::of::<SessionOpen>()]
}
