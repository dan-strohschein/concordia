//! The conformance suite on the in-process test backend.

use std::sync::Arc;

use concordia_script::{ModuleSource, ScriptBackend, ScriptModule, TestBackend};

fn load() -> Arc<dyn ScriptModule> {
    TestBackend::new()
        .link::<conformance_guest::Conformance>()
        .load(&ModuleSource::Linked("conformance".into()))
        .expect("the conformance module is linked")
}

concordia_script::conformance_suite!(load);

/// Every conformance behavior in one run, plus the backend's refusals.
#[test]
fn test_backend_passes_conformance() {
    concordia_script::conformance::run_all(load);
    let backend = TestBackend::new().link::<conformance_guest::Conformance>();
    assert_eq!(backend.name(), "test");
    assert!(matches!(
        backend.load(&ModuleSource::Bytes(Vec::new())),
        Err(concordia_script::ScriptError::Unsupported { .. })
    ));
    assert!(matches!(
        backend.load(&ModuleSource::Linked("nope".into())),
        Err(concordia_script::ScriptError::UnknownModule(_))
    ));
}

#[test]
fn enable_all_gives_faulted_processors_a_fresh_start() {
    let mut rig = concordia_script::conformance::Rig::new(load);
    rig.spawn(&[("conf.Faulty", serde_json::json!(null))]);
    rig.step(1);
    assert!(rig.host.is_disabled("conf.fault"));
    rig.host.enable_all();
    assert!(!rig.host.is_disabled("conf.fault"));
    rig.step(1);
    assert_eq!(rig.host.faults().len(), 2, "it ran (and faulted) again");
}

/// What `emit_logs` sends to tracing, as (level, message) pairs, after `ticks` ticks.
fn emitted(threads: usize, ticks: u32) -> Vec<(String, String)> {
    use tracing_subscriber::layer::SubscriberExt;
    let ring = Arc::new(concordia_core::log::LogRing::new(
        std::num::NonZeroUsize::new(512).unwrap(),
    ));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let mut rig = concordia_script::conformance::Rig::build(load, threads, &[], |_| {});
        rig.spawn(&[("conf.Faulty", serde_json::json!(null))]);
        rig.spawn(&[(
            "conf.WasiProbe",
            serde_json::json!({"wall_ns": 0, "mono_ns": 0, "random": 0}),
        )]);
        for _ in 0..ticks {
            rig.step(1);
            rig.host.emit_logs();
        }
        rig.host.emit_logs();
    });
    ring.tail(None)
        .into_iter()
        .map(|r| (format!("{:?}", r.level), r.message))
        .collect()
}

#[test]
fn emitted_logs_and_faults_come_in_tick_then_processor_order_once() {
    let one = emitted(1, 2);
    let messages: Vec<&str> = one.iter().map(|(_, m)| m.as_str()).collect();
    assert_eq!(
        messages,
        [
            "conf.fault: about to trap",
            "script processor faulted and was disabled"
        ],
        "{one:?}"
    );
    assert_eq!(one, emitted(8, 2), "the same at any thread count");
}
