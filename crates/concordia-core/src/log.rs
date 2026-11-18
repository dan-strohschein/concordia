//! Logging: human-readable output to stderr only, plus an in-memory ring for `log.tail`.
//!
//! Stdout belongs to stdio transports (JSON-RPC, MCP), so nothing here writes to it.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Env var holding the tracing filter (e.g. `info,concordia_terra=debug`).
pub const LOG_ENV: &str = "CONCORDIA_LOG";

/// Errors from logging setup.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// The filter directive did not parse.
    #[error("invalid log filter {filter:?}: {reason}")]
    Filter {
        /// The rejected filter.
        filter: String,
        /// Parser message.
        reason: String,
    },
    /// A global subscriber is already installed.
    #[error("logging is already initialized in this process")]
    AlreadyInitialized,
}

/// Logging configuration.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// `EnvFilter` directive.
    pub filter: String,
    /// Records kept for `log.tail`.
    pub ring_capacity: NonZeroUsize,
}

impl LogConfig {
    /// Default ring capacity.
    pub const DEFAULT_RING_CAPACITY: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

    /// Reads the filter from `CONCORDIA_LOG`, defaulting to `info`.
    pub fn from_env() -> Self {
        Self {
            filter: std::env::var(LOG_ENV).unwrap_or_else(|_| "info".to_owned()),
            ring_capacity: Self::DEFAULT_RING_CAPACITY,
        }
    }
}

/// Access to the installed logger.
#[derive(Debug, Clone)]
pub struct LogHandle {
    ring: Arc<LogRing>,
}

impl LogHandle {
    /// The ring of recent records.
    pub fn ring(&self) -> &Arc<LogRing> {
        &self.ring
    }
}

/// Installs the global logger (stderr + ring). Fails if one is already installed.
pub fn init(cfg: &LogConfig) -> Result<LogHandle, LogError> {
    let filter = EnvFilter::try_new(&cfg.filter).map_err(|e| LogError::Filter {
        filter: cfg.filter.clone(),
        reason: e.to_string(),
    })?;
    let ring = Arc::new(LogRing::new(cfg.ring_capacity));
    let stderr = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(ring.layer())
        .try_init()
        .map_err(|_| LogError::AlreadyInitialized)?;
    Ok(LogHandle { ring })
}

/// Severity of a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    /// Very verbose.
    Trace,
    /// Debugging detail.
    Debug,
    /// Normal operation.
    Info,
    /// Something unexpected but handled.
    Warn,
    /// A failure.
    Error,
}

impl From<tracing::Level> for LogLevel {
    fn from(level: tracing::Level) -> Self {
        match level {
            tracing::Level::TRACE => Self::Trace,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::INFO => Self::Info,
            tracing::Level::WARN => Self::Warn,
            _ => Self::Error,
        }
    }
}

/// One captured log event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    /// Monotonic sequence number, starting at 1.
    pub seq: u64,
    /// Severity.
    pub level: LogLevel,
    /// Event target (usually the module path).
    pub target: String,
    /// The event message.
    pub message: String,
    /// Other fields, formatted as text.
    pub fields: BTreeMap<String, String>,
}

/// Bounded buffer of the most recent records.
///
/// ```
/// use std::{num::NonZeroUsize, sync::Arc};
/// use concordia_core::log::LogRing;
/// use tracing_subscriber::layer::SubscriberExt;
/// let ring = Arc::new(LogRing::new(NonZeroUsize::new(16).unwrap()));
/// let subscriber = tracing_subscriber::registry().with(ring.layer());
/// tracing::subscriber::with_default(subscriber, || tracing::info!("hello"));
/// assert_eq!(ring.tail(None)[0].message, "hello");
/// ```
#[derive(Debug)]
pub struct LogRing {
    capacity: NonZeroUsize,
    state: Mutex<RingState>,
}

#[derive(Debug, Default)]
struct RingState {
    records: VecDeque<LogRecord>,
    next_seq: u64,
}

impl LogRing {
    /// Creates an empty ring holding at most `capacity` records.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            state: Mutex::new(RingState {
                records: VecDeque::with_capacity(capacity.get()),
                next_seq: 1,
            }),
        }
    }

    /// Records with `seq` greater than `since` (all if `None`), oldest first.
    pub fn tail(&self, since: Option<u64>) -> Vec<LogRecord> {
        let floor = since.unwrap_or(0);
        self.lock()
            .records
            .iter()
            .filter(|r| r.seq > floor)
            .cloned()
            .collect()
    }

    /// A tracing layer that feeds this ring.
    pub fn layer(self: &Arc<Self>) -> RingLayer {
        RingLayer(Arc::clone(self))
    }

    fn push(&self, mut record: LogRecord) {
        let mut state = self.lock();
        record.seq = state.next_seq;
        state.next_seq += 1;
        if state.records.len() == self.capacity.get() {
            state.records.pop_front();
        }
        state.records.push_back(record);
    }

    fn lock(&self) -> MutexGuard<'_, RingState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Tracing layer that copies events into a [`LogRing`].
#[derive(Debug, Clone)]
pub struct RingLayer(Arc<LogRing>);

impl<S: tracing::Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        self.0.push(LogRecord {
            seq: 0,
            level: (*meta.level()).into(),
            target: meta.target().to_owned(),
            message: visitor.message,
            fields: visitor.fields,
        });
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: BTreeMap<String, String>,
}

impl FieldVisitor {
    fn insert(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = value;
        } else {
            self.fields.insert(field.name().to_owned(), value);
        }
    }
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field, format!("{value:?}"));
    }
}
