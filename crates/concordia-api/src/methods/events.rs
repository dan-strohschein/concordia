//! `events.subscribe` / `events.unsubscribe`: change feeds pushed as JSON-RPC notifications.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::error::{ApiError, ApiErrorKind};
use crate::events::Topic;
use crate::method::{CallCtx, Method, MethodInfo};
use crate::methods::Empty;

/// Receive `events.event` notifications for the given topics (replaces earlier subscriptions). Needs a transport that can push (WebSocket or in-process); otherwise poll `log.tail`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EventsSubscribe;

/// `events.subscribe` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EventsSubscribeParams {
    /// Topics: `edit` (committed changes, undo, redo), `scene` (opened, new, saved), `sim` (play state and tick).
    pub topics: Vec<Topic>,
}

/// `events.subscribe` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EventsSubscribeResult {
    /// The topics now subscribed.
    pub topics: Vec<Topic>,
}

impl Method for EventsSubscribe {
    const NAME: &'static str = "events.subscribe";
    const MUTATES: bool = false;
    type Params = EventsSubscribeParams;
    type Result = EventsSubscribeResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: EventsSubscribeParams,
    ) -> Result<EventsSubscribeResult, ApiError> {
        if !engine.events.has_sink(ctx.session) {
            return Err(ApiError::new(
                ApiErrorKind::InvalidRequest,
                "this connection's transport cannot push events",
            )
            .suggest([
                "connect over the WebSocket transport, or poll log.tail for changes".to_owned(),
            ]));
        }
        engine.events.subscribe(ctx.session, &p.topics);
        Ok(EventsSubscribeResult { topics: p.topics })
    }
}

/// Stop receiving events on this session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EventsUnsubscribe;

impl Method for EventsUnsubscribe {
    const NAME: &'static str = "events.unsubscribe";
    const MUTATES: bool = false;
    type Params = Empty;
    type Result = Empty;

    fn call(engine: &mut Engine, ctx: &CallCtx, _: Empty) -> Result<Empty, ApiError> {
        engine.events.unsubscribe(ctx.session);
        Ok(Empty {})
    }
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<EventsSubscribe>(),
        MethodInfo::of::<EventsUnsubscribe>(),
    ]
}
