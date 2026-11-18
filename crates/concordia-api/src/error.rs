//! The Engine API error model: JSON-RPC codes plus `data` an agent can act on (ADR-0031).

use serde::Serialize;
use serde_json::{Map, Value, json};

/// Every kind of API error. Each has a code, a default message and at least one suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiErrorKind {
    /// The text is not JSON.
    ParseError,
    /// The JSON is not a valid JSON-RPC 2.0 request.
    InvalidRequest,
    /// No such method.
    MethodNotFound,
    /// Params do not match the method's schema.
    InvalidParams,
    /// An engine bug.
    Internal,
    /// No entity has the given id.
    EntityNotFound,
    /// The component name is not registered.
    ComponentNotRegistered,
    /// A component value is invalid.
    ValueInvalid,
    /// A mutating call has no actor.
    ActorRequired,
    /// The actor string is malformed.
    ActorMalformed,
    /// Another session holds an open transaction.
    TxnConflict,
    /// The undo stack is empty.
    NothingToUndo,
    /// The redo stack is empty.
    NothingToRedo,
    /// A file could not be read or written.
    SceneIo,
    /// A scene file could not be parsed.
    SceneParse,
    /// No scene is open.
    NoSceneOpen,
    /// The open scene has unsaved changes.
    SceneDirty,
    /// There is no play world.
    NotPlaying,
    /// The transaction id is not open.
    TxnNotFound,
    /// No GPU device is available for rendering.
    GpuUnavailable,
}

impl ApiErrorKind {
    /// Every kind (for exhaustive tests and docs).
    pub const ALL: [Self; 20] = [
        Self::ParseError,
        Self::InvalidRequest,
        Self::MethodNotFound,
        Self::InvalidParams,
        Self::Internal,
        Self::EntityNotFound,
        Self::ComponentNotRegistered,
        Self::ValueInvalid,
        Self::ActorRequired,
        Self::ActorMalformed,
        Self::TxnConflict,
        Self::NothingToUndo,
        Self::NothingToRedo,
        Self::SceneIo,
        Self::SceneParse,
        Self::NoSceneOpen,
        Self::SceneDirty,
        Self::NotPlaying,
        Self::TxnNotFound,
        Self::GpuUnavailable,
    ];

    /// The JSON-RPC error code.
    pub fn code(self) -> i32 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::Internal => -32603,
            Self::EntityNotFound => -32001,
            Self::ComponentNotRegistered => -32002,
            Self::ValueInvalid => -32003,
            Self::ActorRequired | Self::ActorMalformed => -32004,
            Self::TxnConflict => -32005,
            Self::NothingToUndo | Self::NothingToRedo => -32006,
            Self::SceneIo => -32007,
            Self::SceneParse => -32008,
            Self::NoSceneOpen => -32009,
            Self::SceneDirty => -32010,
            Self::NotPlaying => -32011,
            Self::TxnNotFound => -32012,
            Self::GpuUnavailable => -32013,
        }
    }

    /// The stable name used in `data.kind`.
    pub fn name(self) -> &'static str {
        match self {
            Self::ParseError => "ParseError",
            Self::InvalidRequest => "InvalidRequest",
            Self::MethodNotFound => "MethodNotFound",
            Self::InvalidParams => "InvalidParams",
            Self::Internal => "Internal",
            Self::EntityNotFound => "EntityNotFound",
            Self::ComponentNotRegistered => "ComponentNotRegistered",
            Self::ValueInvalid => "ValueInvalid",
            Self::ActorRequired => "ActorRequired",
            Self::ActorMalformed => "ActorMalformed",
            Self::TxnConflict => "TxnConflict",
            Self::NothingToUndo => "NothingToUndo",
            Self::NothingToRedo => "NothingToRedo",
            Self::SceneIo => "SceneIo",
            Self::SceneParse => "SceneParse",
            Self::NoSceneOpen => "NoSceneOpen",
            Self::SceneDirty => "SceneDirty",
            Self::NotPlaying => "NotPlaying",
            Self::TxnNotFound => "TxnNotFound",
            Self::GpuUnavailable => "GpuUnavailable",
        }
    }

    fn default_message(self) -> &'static str {
        match self {
            Self::ParseError => "the request is not valid JSON",
            Self::InvalidRequest => "the request is not a valid JSON-RPC 2.0 request",
            Self::MethodNotFound => "no such method",
            Self::InvalidParams => "the params do not match the method's schema",
            Self::Internal => "internal engine error",
            Self::EntityNotFound => "no entity has that id",
            Self::ComponentNotRegistered => "no component with that name is registered",
            Self::ValueInvalid => "the value does not match the component's schema",
            Self::ActorRequired => "this call changes the scene and needs an actor",
            Self::ActorMalformed => "the actor is not of the form <kind>:<name>[#n]",
            Self::TxnConflict => "another session holds an open transaction",
            Self::NothingToUndo => "nothing to undo",
            Self::NothingToRedo => "nothing to redo",
            Self::SceneIo => "a scene or journal file could not be read or written",
            Self::SceneParse => "the scene file could not be parsed",
            Self::NoSceneOpen => "no scene is open",
            Self::SceneDirty => "the open scene has unsaved changes",
            Self::NotPlaying => "there is no play world",
            Self::TxnNotFound => "that transaction is not open",
            Self::GpuUnavailable => "no GPU device is available for rendering",
        }
    }

    fn default_suggestion(self) -> &'static str {
        match self {
            Self::ParseError | Self::InvalidRequest => {
                "send one JSON object per line: {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"scene.info\",\"params\":{}}"
            }
            Self::MethodNotFound => {
                "list methods with schema.get (no params) or read concordia://docs/agent-guide"
            }
            Self::InvalidParams => "check the params against schema.get {\"name\": \"<method>\"}",
            Self::Internal => "retry once; if it persists, report it with log.tail output",
            Self::EntityNotFound => "use query.run {\"with\": [\"Name\"]} to find entity ids",
            Self::ComponentNotRegistered => {
                "list registered components with schema.get (no params)"
            }
            Self::ValueInvalid => {
                "read the component schema with schema.get {\"name\": \"<component>\"}"
            }
            Self::ActorRequired => {
                "pass \"actor\": \"agent:<role>\" or call session.open with an actor"
            }
            Self::ActorMalformed => "use <kind>:<name>[#n], e.g. agent:claude-code",
            Self::TxnConflict => "wait for the holder to commit or abort, then retry",
            Self::NothingToUndo => "see txn.history for what can be undone",
            Self::NothingToRedo => "redo is only available right after txn.undo; see txn.history",
            Self::SceneIo => "check the path is inside the project and writable",
            Self::SceneParse => {
                "fix the file at the reported line and column, or run concordia fmt"
            }
            Self::NoSceneOpen => "call scene.new or scene.open first",
            Self::SceneDirty => "call scene.save first, or pass \"discard\": true",
            Self::NotPlaying => "call sim.step or sim.play to create a play world",
            Self::TxnNotFound => "use the txn id returned by txn.begin; open transactions time out",
            Self::GpuUnavailable => {
                "run `concordia gpu-probe`; rendering needs Metal (macOS) in M1"
            }
        }
    }
}

/// `error.data` of a JSON-RPC error response.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorData {
    /// The error kind name.
    pub kind: &'static str,
    /// What to try next, best first.
    pub suggestions: Vec<String>,
    /// JSON Pointer to the offending value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer: Option<String>,
    /// The schema the value must match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    /// A valid example value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
    /// An MCP resource or doc to read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub see: Option<String>,
    /// Kind-specific fields (e.g. `holder`, `line`, `col`).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An Engine API error.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct ApiError {
    /// The kind.
    pub kind: ApiErrorKind,
    /// Human- and agent-readable message.
    pub message: String,
    /// Structured details (boxed to keep `Result<_, ApiError>` small).
    pub data: Box<ErrorData>,
}

impl ApiError {
    /// An error with `message`; `data` starts with the kind's default suggestion.
    pub fn new(kind: ApiErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            data: Box::new(ErrorData {
                kind: kind.name(),
                suggestions: vec![kind.default_suggestion().to_owned()],
                pointer: None,
                expected: None,
                example: None,
                see: None,
                extra: Map::new(),
            }),
        }
    }

    /// An error with the kind's default message.
    pub fn from_kind(kind: ApiErrorKind) -> Self {
        Self::new(kind, kind.default_message())
    }

    /// The JSON-RPC code.
    pub fn code(&self) -> i32 {
        self.kind.code()
    }

    /// Puts `suggestions` before the default one.
    pub fn suggest(mut self, suggestions: impl IntoIterator<Item = String>) -> Self {
        let mut first: Vec<String> = suggestions.into_iter().collect();
        first.append(&mut self.data.suggestions);
        self.data.suggestions = first;
        self
    }

    /// Sets `data.see`.
    pub fn see(mut self, uri: impl Into<String>) -> Self {
        self.data.see = Some(uri.into());
        self
    }

    /// Sets `data.pointer`.
    pub fn at(mut self, pointer: impl Into<String>) -> Self {
        self.data.pointer = Some(pointer.into());
        self
    }

    /// Adds a kind-specific field to `data`.
    pub fn with(mut self, key: &str, value: Value) -> Self {
        self.data.extra.insert(key.to_owned(), value);
        self
    }

    /// The JSON-RPC `error` object.
    pub fn to_jsonrpc(&self) -> Value {
        json!({ "code": self.code(), "message": self.message, "data": self.data })
    }
}
