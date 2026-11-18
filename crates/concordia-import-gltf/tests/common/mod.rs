//! glTF sample assets (fetched by `just gltf-assets`) imported into temporary projects.
#![allow(dead_code, missing_docs)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use concordia_assets::{AssetDb, AssetRef, ImportReport};
use concordia_core::id::DeterministicIdGen;
use concordia_import_gltf::GltfImporter;

const MANIFEST: &str = include_str!("../../../../tests/fixtures/gltf-sample-assets/manifest.ron");

/// `target/gltf-sample-assets/<commit>/Models`.
pub fn samples() -> PathBuf {
    let commit = MANIFEST
        .lines()
        .find_map(|l| l.trim().strip_prefix("commit: \"")?.strip_suffix("\","))
        .expect("manifest has a commit");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/gltf-sample-assets")
        .join(commit)
        .join("Models");
    assert!(
        dir.is_dir(),
        "glTF sample assets are missing: run `just gltf-assets` ({})",
        dir.display()
    );
    dir
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub struct Project {
    pub dir: tempfile::TempDir,
    pub db: AssetDb,
}

impl Project {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut db = AssetDb::new(dir.path(), Box::new(DeterministicIdGen::new(9))).unwrap();
        db.register(Arc::new(GltfImporter));
        Self { dir, db }
    }

    /// Copies every file of `Models/<model>/<variant>/` into the project and imports `file`.
    pub fn sample(&mut self, model: &str, variant: &str, file: &str) -> ImportReport {
        let src = samples().join(model).join(variant);
        let dst = self.dir.path().join(model).join(variant);
        std::fs::create_dir_all(&dst).unwrap();
        for entry in std::fs::read_dir(&src).unwrap().flatten() {
            std::fs::copy(entry.path(), dst.join(entry.file_name())).unwrap();
        }
        self.db
            .import(&Path::new(model).join(variant).join(file))
            .unwrap()
    }

    /// Copies a hand-made fixture into the project and imports it.
    pub fn fixture(&mut self, name: &str) -> Result<ImportReport, concordia_assets::AssetError> {
        std::fs::copy(fixture(name), self.dir.path().join(name)).unwrap();
        self.db.import(Path::new(name))
    }

    pub fn sub(report: &ImportReport, path: &str) -> AssetRef {
        assert!(
            report.sub_assets.contains_key(path),
            "no {path} in {:?}",
            report.sub_assets.keys()
        );
        format!("asset:{}#{path}", report.id).parse().unwrap()
    }
}
