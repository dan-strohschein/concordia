//! JSON-RPC 2.0 framing: single requests, batches, notifications and protocol errors.

use concordia_core::id::SessionId;
use serde_json::{Value, json};

use crate::engine::Engine;
use crate::error::{ApiError, ApiErrorKind};

impl Engine {
    /// Handles one JSON-RPC message (request, notification or batch); `None` if nothing should be sent back.
    pub fn handle_jsonrpc(&mut self, session: SessionId, text: &str) -> Option<String> {
        let message: Value = match serde_json::from_str(text) {
            Ok(message) => message,
            Err(e) => {
                return Some(
                    error_response(
                        &Value::Null,
                        &ApiError::new(ApiErrorKind::ParseError, e.to_string()),
                    )
                    .to_string(),
                );
            }
        };
        match message {
            Value::Array(items) if items.is_empty() => Some(
                error_response(
                    &Value::Null,
                    &ApiError::new(ApiErrorKind::InvalidRequest, "empty batch"),
                )
                .to_string(),
            ),
            Value::Array(items) => {
                let responses: Vec<Value> = items
                    .into_iter()
                    .filter_map(|item| self.request(session, item))
                    .collect();
                (!responses.is_empty()).then(|| Value::Array(responses).to_string())
            }
            single => self.request(session, single).map(|r| r.to_string()),
        }
    }

    fn request(&mut self, session: SessionId, request: Value) -> Option<Value> {
        let id = request.get("id").cloned();
        let parsed = parse_request(&request);
        let (method, params) = match parsed {
            Ok(parts) => parts,
            Err(e) => return Some(error_response(&id.unwrap_or(Value::Null), &e)),
        };
        let outcome = self.call(session, &method, params);
        let id = id?;
        Some(match outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => error_response(&id, &e),
        })
    }
}

fn parse_request(request: &Value) -> Result<(String, Value), ApiError> {
    let invalid = |why: &str| ApiError::new(ApiErrorKind::InvalidRequest, why.to_owned());
    let object = request
        .as_object()
        .ok_or_else(|| invalid("a request must be a JSON object"))?;
    if object.get("jsonrpc") != Some(&json!("2.0")) {
        return Err(invalid("\"jsonrpc\" must be \"2.0\""));
    }
    if object
        .get("id")
        .is_some_and(|id| !(id.is_string() || id.is_number() || id.is_null()))
    {
        return Err(invalid("\"id\" must be a string, number or null"));
    }
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("\"method\" must be a string"))?;
    let params = match object.get("params") {
        None | Some(Value::Null) => Value::Null,
        Some(params @ Value::Object(_)) => params.clone(),
        Some(_) => {
            return Err(ApiError::new(
                ApiErrorKind::InvalidParams,
                "params must be an object (named parameters)",
            ));
        }
    };
    Ok((method.to_owned(), params))
}

fn error_response(id: &Value, error: &ApiError) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": error.to_jsonrpc() })
}
