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
