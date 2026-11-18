//! `log.tail`.

use std::collections::BTreeMap;

use concordia_core::log::LogLevel;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::error::ApiError;
use crate::method::{CallCtx, Method, MethodInfo};

/// Read recent engine log records (stderr logs are also kept in memory for this).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LogTail;

/// Minimum level.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Level {
    /// Everything.
    trace,
    /// Debug and above.
    debug,
    /// Info and above.
    info,
    /// Warnings and errors.
    warn,
    /// Errors only.
    error,
}

/// `log.tail` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct LogTailParams {
    /// Only records after this sequence number (use `next` from the last call).
    pub since: Option<u64>,
    /// Minimum level (default trace).
    pub level: Option<Level>,
    /// Maximum records (default 100).
    #[reflect(range(min = 1.0, max = 10000.0))]
    pub limit: Option<u32>,
}

/// One log record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LogRow {
    /// Sequence number.
    pub seq: u64,
    /// "trace" … "error".
    pub level: String,
    /// Event target (module).
    pub target: String,
    /// Message.
    pub message: String,
    /// Structured fields.
    pub fields: BTreeMap<String, String>,
}

/// `log.tail` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LogTailResult {
    /// Records, oldest first.
    pub records: Vec<LogRow>,
    /// Pass as `since` to continue.
    pub next: u64,
}

impl Method for LogTail {
    const NAME: &'static str = "log.tail";
    const MUTATES: bool = false;
    type Params = LogTailParams;
    type Result = LogTailResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: LogTailParams) -> Result<LogTailResult, ApiError> {
        let min = p.level.map_or(LogLevel::Trace, level);
        let all = engine.ring.tail(p.since);
        let next = all.last().map_or(p.since.unwrap_or(0), |r| r.seq);
        let records: Vec<LogRow> = all
            .into_iter()
            .filter(|r| r.level >= min)
            .take(p.limit.unwrap_or(100) as usize)
            .map(|r| LogRow {
                seq: r.seq,
                level: name(r.level).into(),
                target: r.target,
                message: r.message,
                fields: r.fields,
            })
            .collect();
        let next = match records.len() == p.limit.unwrap_or(100) as usize {
            true => records.last().map_or(next, |r| r.seq),
            false => next,
        };
        Ok(LogTailResult { records, next })
    }
}

fn level(level: Level) -> LogLevel {
    match level {
        Level::trace => LogLevel::Trace,
        Level::debug => LogLevel::Debug,
        Level::info => LogLevel::Info,
        Level::warn => LogLevel::Warn,
        Level::error => LogLevel::Error,
    }
}

fn name(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![MethodInfo::of::<LogTail>()]
}
