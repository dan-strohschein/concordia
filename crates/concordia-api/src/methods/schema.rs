//! `schema.get`.

use concordia_core::suggest::did_you_mean;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::Engine;
use crate::error::{ApiError, ApiErrorKind};
use crate::method::{CallCtx, Method, MethodInfo, params_schema, result_schema};

/// Get the JSON Schema of a component, type or method; with no name, the index of all types and methods.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SchemaGet;

/// `schema.get` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SchemaGetParams {
    /// A type name (e.g. "Transform") or method name (e.g. "entity.spawn").
    pub name: Option<String>,
}

/// `schema.get` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SchemaGetResult {
    /// The schema, method description or index.
    pub schema: Value,
}

impl Method for SchemaGet {
    const NAME: &'static str = "schema.get";
    const MUTATES: bool = false;
    type Params = SchemaGetParams;
    type Result = SchemaGetResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: SchemaGetParams,
    ) -> Result<SchemaGetResult, ApiError> {
        let schema = match p.name {
            None => {
                let mut index = engine.registry.schema_index();
                index["methods"] = json!(engine.methods.names().collect::<Vec<_>>());
                index
            }
            Some(name) => match engine.methods.get(&name) {
                Some(method) => json!({
                    "method": method.name,
                    "mutates": method.mutates,
                    "description": method.docs,
                    "params": params_schema(method),
                    "result": result_schema(method),
                }),
                None => engine
                    .registry
                    .schema_for(&name)
                    .ok_or_else(|| unknown(engine, &name))?,
            },
        };
        Ok(SchemaGetResult { schema })
    }
}

fn unknown(engine: &Engine, name: &str) -> ApiError {
    let candidates: Vec<&str> = engine
        .registry
        .names()
        .chain(engine.methods.names())
        .collect();
    let similar = did_you_mean(name, candidates)
        .into_iter()
        .map(|n| format!("did you mean \"{n}\"?"));
    ApiError::new(
        ApiErrorKind::ComponentNotRegistered,
        format!("no type or method named {name:?}"),
    )
    .suggest(similar)
    .see("concordia://schema/index")
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![MethodInfo::of::<SchemaGet>()]
}
