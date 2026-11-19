//! Log ring for `log.tail` and stdout hygiene (M0 phase 0.1).

use std::num::NonZeroUsize;
use std::process::Command;
use std::sync::Arc;

use concordia_core::log::{LogConfig, LogLevel, LogRing, init};
use tracing_subscriber::layer::SubscriberExt;

const CHILD_ENV: &str = "CONCORDIA_LOG_PROBE_CHILD";

fn ring(capacity: usize) -> Arc<LogRing> {
    Arc::new(LogRing::new(NonZeroUsize::new(capacity).unwrap()))
}

fn with_ring(ring: &Arc<LogRing>, f: impl FnOnce()) {
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, f);
}

fn seqs(ring: &LogRing, since: Option<u64>) -> Vec<u64> {
    ring.tail(since).iter().map(|r| r.seq).collect()
}

#[test]
fn log_ring_keeps_last_n_records_with_monotonic_seq() {
    let ring = ring(4);
    with_ring(&ring, || (1..=10).for_each(|i| tracing::info!(i, "event")));
    assert_eq!(seqs(&ring, None), vec![7, 8, 9, 10]);
}

#[test]
fn log_ring_tail_since_returns_only_newer() {
    let ring = ring(4);
    with_ring(&ring, || (1..=10).for_each(|i| tracing::info!(i, "event")));
    assert_eq!(seqs(&ring, Some(8)), vec![9, 10]);
    assert!(seqs(&ring, Some(10)).is_empty());
}

#[test]
fn log_ring_records_level_target_message_and_fields() {
    let ring = ring(8);
    with_ring(
        &ring,
        || tracing::warn!(target: "scene", entities = 3, "scene saved"),
    );
    let record = &ring.tail(None)[0];
    assert_eq!(record.level, LogLevel::Warn);
    assert_eq!(record.target, "scene");
    assert_eq!(record.message, "scene saved");
    assert_eq!(record.fields.get("entities").map(String::as_str), Some("3"));
}

#[test]
fn log_init_never_writes_to_stdout() {
    if std::env::var_os(CHILD_ENV).is_some() {
        emit_all_levels_after_init();
        return;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["log_init_never_writes_to_stdout", "--exact", "--nocapture"])
        .env(CHILD_ENV, "1")
        .env("CONCORDIA_LOG", "trace")
        .output()
        .unwrap();
    assert!(output.status.success(), "child failed: {output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for level in ["trace", "debug", "info", "warn", "error"] {
        let marker = format!("probe-{level}-marker");
        assert!(!stdout.contains(&marker), "{marker} leaked to stdout");
        assert!(stderr.contains(&marker), "{marker} missing from stderr");
    }
}

fn emit_all_levels_after_init() {
    let handle = init(&LogConfig::from_env()).expect("first init succeeds");
    tracing::trace!("probe-trace-marker");
    tracing::debug!("probe-debug-marker");
    tracing::info!("probe-info-marker");
    tracing::warn!("probe-warn-marker");
    tracing::error!("probe-error-marker");
    assert_eq!(handle.ring().tail(None).len(), 5);
    assert!(
        init(&LogConfig::from_env()).is_err(),
        "second init is an error"
    );
}

#[test]
fn the_ring_reports_its_next_sequence_number_without_copying() {
    use std::num::NonZeroUsize;
    use tracing_subscriber::layer::SubscriberExt;
    let ring = std::sync::Arc::new(concordia_core::log::LogRing::new(
        NonZeroUsize::new(4).unwrap(),
    ));
    assert_eq!(ring.next_seq(), 1);
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || tracing::info!("one"));
    assert_eq!(ring.next_seq(), 2);
}
