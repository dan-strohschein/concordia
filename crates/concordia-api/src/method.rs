//! Typed methods, the method registry and dispatch.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use concordia_command::Actor;
use concordia_core::id::SessionId;
use concordia_core::suggest::did_you_mean;
use concordia_reflect::{Reflect, TypeInfo, schema_for, validate_json};
use serde_json::{Value, json};

use crate::engine::Engine;
use crate::error::{ApiError, ApiErrorKind};
use crate::module::{CoreModule, EngineModule};

/// Who a method is for: agents (offered as an MCP tool) or the editor's own plumbing (WebSocket API only).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Audience {
    /// Agents and every other client.
    #[default]
    agent,
    /// The editor's viewport plumbing; not offered to agents.
    editor,
}

/// One Engine API method. The marker type's doc comment is the method's agent-facing description.
pub trait Method: Reflect {
    /// Wire name, `domain.verb`.
    const NAME: &'static str;
    /// True if the method changes engine or project state (edit world, tasks); requires an actor.
    const MUTATES: bool;
    /// Who the method is for (default: agents).
    const AUDIENCE: Audience = Audience::agent;
    /// Params.
    type Params: Reflect;
    /// Result.
    type Result: Reflect;
    /// Runs the method.
    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        params: Self::Params,
    ) -> Result<Self::Result, ApiError>;
}

/// Who is calling.
#[derive(Debug, Clone)]
pub struct CallCtx {
    /// The session.
    pub session: SessionId,
    /// The resolved actor (explicit param or session default); always set for mutating methods.
    pub actor: Option<Actor>,
    /// When the call started.
    pub now: Instant,
}

impl CallCtx {
    /// The actor; mutating methods can rely on it.
    pub fn actor(&self) -> Result<&Actor, ApiError> {
        self.actor
            .as_ref()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::ActorRequired))
    }
}

type Invoke = fn(&mut Engine, &CallCtx, Value) -> Result<Value, ApiError>;

/// A registered method.
#[derive(Clone, Copy)]
pub struct MethodInfo {
    /// Wire name.
    pub name: &'static str,
    /// Requires an actor.
    pub mutates: bool,
    /// Agent-facing description.
    pub docs: &'static str,
    /// Who the method is for.
    pub audience: Audience,
    /// Params type.
    pub params: &'static TypeInfo,
    /// Result type.
    pub result: &'static TypeInfo,
    invoke: Invoke,
}

impl std::fmt::Debug for MethodInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MethodInfo")
            .field("name", &self.name)
            .field("mutates", &self.mutates)
            .finish()
    }
}

impl MethodInfo {
    /// Describes method `M`.
    pub fn of<M: Method>() -> Self {
        Self {
            name: M::NAME,
            mutates: M::MUTATES,
            docs: M::type_info().docs,
            audience: M::AUDIENCE,
            params: M::Params::type_info(),
            result: M::Result::type_info(),
            invoke: invoke::<M>,
        }
    }

    /// The MCP tool name (`entity.spawn` → `entity_spawn`).
    pub fn tool_name(&self) -> String {
        self.name.replace('.', "_")
    }

    /// True for methods offered to agents as MCP tools: agent audience, except `session.open`
    /// (the adapter opens the session) and `events.*` (they need a push channel tool calls lack).
    pub fn is_agent_tool(&self) -> bool {
        self.audience == Audience::agent
            && self.name != "session.open"
            && !self.name.starts_with("events.")
    }
}

/// All methods, sorted by name.
#[derive(Debug)]
pub struct MethodRegistry {
    methods: Vec<MethodInfo>,
}

impl MethodRegistry {
    /// Collects every module's methods, sorted by name; a name provided twice is an error.
    pub fn build(modules: &[Arc<dyn EngineModule>]) -> Result<Self, ApiError> {
        let mut owners: BTreeMap<&'static str, (&'static str, MethodInfo)> = BTreeMap::new();
        for module in modules {
            for method in module.methods() {
                if let Some((owner, _)) = owners.insert(method.name, (module.name(), method)) {
                    return Err(ApiError::new(
                        ApiErrorKind::Internal,
                        format!(
                            "method {} is provided by both module {owner} and module {}",
                            method.name,
                            module.name()
                        ),
                    ));
                }
            }
        }
        Ok(Self {
            methods: owners.into_values().map(|(_, m)| m).collect(),
        })
    }

    /// The core methods plus `modules`' methods: the set an engine with those modules serves.
    pub fn with_core(modules: &[Arc<dyn EngineModule>]) -> Result<Self, ApiError> {
        Self::build(&crate::module::with_core(modules))
    }

    /// Looks up a method.
    pub fn get(&self, name: &str) -> Option<&MethodInfo> {
        self.methods
            .binary_search_by(|m| m.name.cmp(name))
            .ok()
            .map(|i| &self.methods[i])
    }

    /// Methods in name order.
    pub fn iter(&self) -> impl Iterator<Item = &MethodInfo> {
        self.methods.iter()
    }

    /// Method names in order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.methods.iter().map(|m| m.name)
    }
}

/// The core (M0) methods; an engine's full set, including modules, is [`Engine::methods`].
pub fn methods() -> &'static MethodRegistry {
    static REGISTRY: OnceLock<MethodRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let core: Arc<dyn EngineModule> = Arc::new(CoreModule);
        MethodRegistry::build(&[core]).expect("core method names are unique")
    })
}

/// JSON Schema of a method's params (generated, never hand-written).
pub fn params_schema(method: &MethodInfo) -> Value {
    schema_for(method.params)
}

/// JSON Schema of a method's result.
pub fn result_schema(method: &MethodInfo) -> Value {
    schema_for(method.result)
}

/// Resolves `method` and runs it.
pub(crate) fn dispatch(
    engine: &mut Engine,
    session: SessionId,
    method: &str,
    params: Value,
) -> Result<Value, ApiError> {
    let info = *engine.methods.get(method).ok_or_else(|| {
        let similar = did_you_mean(method, engine.methods.names())
            .into_iter()
            .map(|m| format!("did you mean {m}?"));
        ApiError::new(
            ApiErrorKind::MethodNotFound,
            format!("no method named {method:?}"),
        )
        .suggest(similar)
        .see("concordia://docs/agent-guide")
    })?;
    let params = if params.is_null() { json!({}) } else { params };
    let explicit = explicit_actor(&params)?;
    let default = engine.sessions.get(&session).and_then(|s| s.actor.clone());
    let actor = explicit.or(default);
    if info.mutates && actor.is_none() {
        return Err(ApiError::from_kind(ApiErrorKind::ActorRequired));
    }
    let ctx = CallCtx {
        session,
        actor,
        now: Instant::now(),
    };
    (info.invoke)(engine, &ctx, params)
}

fn explicit_actor(params: &Value) -> Result<Option<Actor>, ApiError> {
    match params.get("actor") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            text.parse()
                .map(Some)
                .map_err(|e: concordia_command::ActorError| {
                    let concordia_command::ActorError::Malformed { hint, .. } = &e;
                    ApiError::new(ApiErrorKind::ActorMalformed, e.to_string())
                        .suggest([hint.clone()])
                        .at("/actor")
                })
        }
        Some(_) => {
            Err(ApiError::new(ApiErrorKind::ActorMalformed, "actor must be a string").at("/actor"))
        }
    }
}

fn invoke<M: Method>(engine: &mut Engine, ctx: &CallCtx, params: Value) -> Result<Value, ApiError> {
    let params = decode::<M::Params>(params, M::NAME)?;
    let result = M::call(engine, ctx, params)?;
    serde_json::to_value(result).map_err(crate::engine::internal)
}

fn decode<P: Reflect>(params: Value, method: &str) -> Result<P, ApiError> {
    validate_json(&params, P::type_info())
        .map_err(|e| crate::map::invalid_params(&e, P::type_info(), method))?;
    serde_json::from_value(params).map_err(|e| {
        ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("invalid params for {method}: {e}"),
        )
        .see(format!("concordia://schema/method/{method}"))
    })
}
