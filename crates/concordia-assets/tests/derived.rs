//! Importers see the DDC as a cache for derived sub-results, and share one job pool (M4 4.2).

mod common;

use std::sync::{Arc, Mutex};

use common::Project;
use concordia_assets::{
    Artifact, AssetError, AssetKind, DerivedCache, ImportInput, ImportOutput, Importer,
    NoDerivedCache, import_jobs,
};
use concordia_core::ddc::DdcKey;

/// Builds one derived blob per source, reusing it through `input.derived`; records hits.
struct Deriving(Arc<Mutex<Vec<bool>>>);

impl Importer for Deriving {
    fn name(&self) -> &'static str {
        "deriving"
    }
    fn version(&self) -> u32 {
        1
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["drv"]
    }
    fn default_settings(&self) -> serde_json::Value {
        serde_json::json!({ "unrelated": 0 })
    }
    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        assert!(input.jobs.threads().get() >= 1);
        let key = DdcKey::of(&[b"derived-test", input.bytes]);
        let hit = input.derived.get(&key)?.is_some();
        if !hit {
            input.derived.put(&key, b"built")?;
        }
        self.0.lock().unwrap().push(hit);
        let mut out = ImportOutput::default();
        out.artifacts.insert(
            String::new(),
            Artifact {
                kind: AssetKind::Mesh,
                bytes: vec![],
            },
        );
        Ok(out)
    }
}

#[test]
fn importers_reuse_derived_results_across_setting_changes() {
    let mut project = Project::new();
    let hits = Arc::new(Mutex::new(Vec::new()));
    project.db.register(Arc::new(Deriving(hits.clone())));
    std::fs::write(project.path("a.drv"), b"source").unwrap();
    project.db.import("a.drv".as_ref()).unwrap();
    let sidecar = project.path("a.drv.import.ron");
    let text = std::fs::read_to_string(&sidecar)
        .unwrap()
        .replace("\"unrelated\": 0", "\"unrelated\": 1");
    std::fs::write(&sidecar, text).unwrap();
    let second = project.db.import("a.drv".as_ref()).unwrap();
    assert!(!second.cached, "the setting changed the import key");
    assert_eq!(
        *hits.lock().unwrap(),
        [false, true],
        "the derived blob was reused"
    );
}

#[test]
fn import_jobs_is_one_shared_pool() {
    let a = import_jobs().unwrap() as *const _;
    let b = import_jobs().unwrap() as *const _;
    assert_eq!(a, b);
}

#[test]
fn no_derived_cache_never_hits() {
    let key = DdcKey::of(&[b"x"]);
    NoDerivedCache.put(&key, b"y").unwrap();
    assert_eq!(NoDerivedCache.get(&key).unwrap(), None);
}
