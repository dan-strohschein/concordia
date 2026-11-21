//! Imported fixtures (our FBX rig and Khronos glTF samples) and hand-built skeletons and clips.
#![allow(dead_code, missing_docs)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use concordia_anim::{Joint, Skeleton};
use concordia_assets::{
    AssetDb, AssetKind, AssetRef, Channel, ChannelPath, ClipData, ImportReport, Interpolation,
    SkinData,
};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_import_fbx::FbxImporter;
use concordia_import_gltf::GltfImporter;
use concordia_scene::PrefabDoc;
use concordia_scene::builtin::Transform;

const MANIFEST: &str = include_str!("../../../../tests/fixtures/gltf-sample-assets/manifest.ron");

/// `target/gltf-sample-assets/<commit>/Models` (fetched by `just gltf-assets`).
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
        "glTF sample assets are missing: run `just gltf-assets`"
    );
    dir
}

pub fn fbx_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fbx")
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
        db.register(Arc::new(FbxImporter));
        Self { dir, db }
    }

    pub fn fbx(&mut self, name: &str) -> ImportReport {
        std::fs::copy(fbx_fixture(name), self.dir.path().join(name)).unwrap();
        self.db.import(Path::new(name)).unwrap()
    }

    pub fn gltf(&mut self, model: &str, variant: &str, file: &str) -> ImportReport {
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

    pub fn sub(report: &ImportReport, path: &str) -> AssetRef {
        assert!(
            report.sub_assets.contains_key(path),
            "no {path} in {:?}",
            report.sub_assets.keys()
        );
        format!("asset:{}#{path}", report.id).parse().unwrap()
    }

    pub fn prefab(&mut self, report: &ImportReport) -> PrefabDoc {
        concordia_import_model::load_prefab_doc(&mut self.db, report).unwrap()
    }

    pub fn skin(&mut self, report: &ImportReport) -> SkinData {
        self.db
            .load(&Self::sub(report, "skin/0"), AssetKind::Skin)
            .unwrap()
    }

    pub fn clip(&mut self, report: &ImportReport, sub: &str) -> ClipData {
        self.db
            .load(&Self::sub(report, sub), AssetKind::Clip)
            .unwrap()
    }

    pub fn skeleton(&mut self, report: &ImportReport) -> Skeleton {
        let skin = self.skin(report);
        let prefab = self.prefab(report);
        Skeleton::from_skin(&skin, &prefab).unwrap()
    }

    /// The prefab entity holding a `MeshRenderer` with the `Skinned` component.
    pub fn skinned_entity(&mut self, report: &ImportReport) -> EntityId {
        let doc = self.prefab(report);
        doc.entities
            .iter()
            .find(|e| e.components.contains_key("Skinned"))
            .expect("a skinned entity")
            .id
    }
}

/// A joint at `translation` with no offset and an identity inverse bind.
pub fn joint(name: &str, parent: Option<usize>, translation: Vec3) -> Joint {
    Joint {
        name: name.to_owned(),
        parent,
        offset: Mat4::IDENTITY,
        rest: Transform {
            translation,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        inverse_bind: Mat4::IDENTITY,
    }
}

/// A one-channel clip on bone `bone`.
pub fn clip(
    bone: &str,
    path: ChannelPath,
    interpolation: Interpolation,
    times: &[f32],
    values: &[f32],
) -> ClipData {
    ClipData {
        name: "test".to_owned(),
        duration: times.last().copied().unwrap_or(0.0),
        frame_rate: None,
        channels: vec![Channel {
            target: "018bcfe5-6800-7000-a424-6c55076b137f".parse().unwrap(),
            target_name: bone.to_owned(),
            path,
            interpolation,
            times: times.to_vec(),
            values: values.to_vec(),
        }],
    }
}

pub fn close(a: Vec3, b: Vec3, eps: f32) -> bool {
    (a - b).abs().max_element() <= eps
}
