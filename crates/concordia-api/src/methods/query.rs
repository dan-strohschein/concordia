//! `query.run`.

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use concordia_terra::{DynamicQuery, World};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{WorldSel, require_component, world_of};
use crate::engine::Engine;
use crate::error::{ApiError, ApiErrorKind};
use crate::map::{self, Hints};
use crate::method::{CallCtx, Method, MethodInfo};

const DEFAULT_LIMIT: u32 = 100;

/// Find entities by components and field conditions; rows are sorted by id and paged with a cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct QueryRun;

/// Comparison operator.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum CmpOp {
    /// Equal (any JSON value).
    eq,
    /// Not equal.
    ne,
    /// Less than (numbers or strings).
    lt,
    /// Less than or equal.
    le,
    /// Greater than.
    gt,
    /// Greater than or equal.
    ge,
    /// Substring of a string, or element of an array.
    contains,
}

/// A field condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    /// Component name then JSON Pointer, e.g. "Transform/translation/0" or "Name".
    pub path: String,
    /// Operator.
    pub op: CmpOp,
    /// Value to compare with.
    pub value: Value,
}

/// `query.run` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct QueryRunParams {
    /// Components every match has.
    pub with: Vec<String>,
    /// Components no match has.
    pub without: Option<Vec<String>>,
    /// Conditions that must all hold (entities missing the field do not match).
    pub r#where: Option<Vec<Condition>>,
    /// Components to return per row (default: all).
    pub select: Option<Vec<String>>,
    /// "edit" (default) or "play".
    pub world: Option<WorldSel>,
    /// Maximum rows (1 to 1000, default 100).
    #[reflect(range(min = 1.0, max = 1000.0))]
    pub limit: Option<u32>,
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
}

/// One result row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Row {
    /// Entity id.
    pub id: EntityId,
    /// Selected component values.
    pub components: BTreeMap<String, Value>,
}

/// `query.run` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct QueryRunResult {
    /// Matching rows, sorted by id.
    pub rows: Vec<Row>,
    /// Pass as `cursor` to get the next page; absent on the last page.
    pub next_cursor: Option<String>,
}

impl Method for QueryRun {
    const NAME: &'static str = "query.run";
    const MUTATES: bool = false;
    type Params = QueryRunParams;
    type Result = QueryRunResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: QueryRunParams,
    ) -> Result<QueryRunResult, ApiError> {
        let conditions = parse_conditions(engine, p.r#where.unwrap_or_default())?;
        for name in p.select.iter().flatten() {
            require_component(engine, name)?;
        }
        let world = world_of(engine, p.world)?;
        let query = DynamicQuery {
            with: p.with,
            without: p.without.unwrap_or_default(),
        };
        let ids = query.run(world, &engine.registry).map_err(|e| {
            map::ecs(
                e,
                &Hints {
                    registry: &engine.registry,
                    world: Some(world),
                },
            )
        })?;
        let after: Option<EntityId> =
            p.cursor
                .as_deref()
                .map(str::parse)
                .transpose()
                .map_err(|_| {
                    ApiError::new(
                        ApiErrorKind::InvalidParams,
                        "cursor must be a next_cursor from a previous page",
                    )
                    .at("/cursor")
                })?;
        let limit = p.limit.unwrap_or(DEFAULT_LIMIT) as usize;
        let mut matches = ids
            .into_iter()
            .filter(|id| after.is_none_or(|a| *id > a))
            .filter(|id| conditions.iter().all(|c| c.holds(world, *id)));
        let rows = matches
            .by_ref()
            .take(limit)
            .map(|id| row(world, id, p.select.as_deref()))
            .collect::<Vec<_>>();
        let next_cursor = match matches.next() {
            Some(_) => rows.last().map(|r| r.id.to_string()),
            None => None,
        };
        Ok(QueryRunResult { rows, next_cursor })
    }
}

struct Parsed {
    component: String,
    pointer: String,
    op: CmpOp,
    value: Value,
}

impl Parsed {
    fn holds(&self, world: &World, id: EntityId) -> bool {
        let Ok(json) = world.get_json(id, &self.component) else {
            return false;
        };
        json.pointer(&self.pointer)
            .is_some_and(|actual| compare(actual, self.op, &self.value))
    }
}

fn parse_conditions(engine: &Engine, conditions: Vec<Condition>) -> Result<Vec<Parsed>, ApiError> {
    conditions
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let (component, rest) = c.path.split_once('/').unwrap_or((c.path.as_str(), ""));
            if component.is_empty() {
                return Err(ApiError::new(
                    ApiErrorKind::InvalidParams,
                    "a where path starts with a component name, e.g. \"Transform/translation/0\"",
                )
                .at(format!("/where/{i}/path")));
            }
            require_component(engine, component)?;
            let pointer = if rest.is_empty() {
                String::new()
            } else {
                format!("/{rest}")
            };
            Ok(Parsed {
                component: component.to_owned(),
                pointer,
                op: c.op,
                value: c.value,
            })
        })
        .collect()
}

fn compare(actual: &Value, op: CmpOp, expected: &Value) -> bool {
    use std::cmp::Ordering::{Equal, Greater, Less};
    let order = match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => a
            .as_f64()
            .zip(b.as_f64())
            .and_then(|(a, b)| a.partial_cmp(&b)),
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        _ => None,
    };
    match op {
        CmpOp::eq => order.map_or(actual == expected, |o| o == Equal),
        CmpOp::ne => order.map_or(actual != expected, |o| o != Equal),
        CmpOp::lt => order == Some(Less),
        CmpOp::le => matches!(order, Some(Less | Equal)),
        CmpOp::gt => order == Some(Greater),
        CmpOp::ge => matches!(order, Some(Greater | Equal)),
        CmpOp::contains => match (actual, expected) {
            (Value::String(a), Value::String(b)) => a.contains(b.as_str()),
            (Value::Array(items), _) => items.contains(expected),
            _ => false,
        },
    }
}

fn row(world: &World, id: EntityId, select: Option<&[String]>) -> Row {
    let names: Vec<String> = match select {
        Some(names) => names.to_vec(),
        None => world
            .component_names(id)
            .unwrap_or_default()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };
    let components = names
        .into_iter()
        .filter_map(|name| world.get_json(id, &name).ok().map(|v| (name, v)))
        .collect();
    Row { id, components }
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![MethodInfo::of::<QueryRun>()]
}
