//! Damaged FBX files are an error or a smaller import, never a panic or a crash.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use concordia_assets::{AssetError, ImportInput, ImportOutput, Importer, SubIds};
use concordia_core::id::{AssetId, TypedId};
use concordia_import_fbx::FbxImporter;
use proptest::prelude::*;

static SOURCES: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    ["static_quad.fbx", "skinned.fbx", "mixamo_run.fbx"]
        .map(|f| std::fs::read(common::fixture(f)).unwrap())
        .to_vec()
});

fn import(bytes: &[u8]) -> Result<ImportOutput, AssetError> {
    let parent = AssetId::try_from_uuid(uuid_v5()).unwrap();
    let mut sub_ids = SubIds::new(parent, BTreeMap::new());
    let read_relative = |name: &str| -> Result<Vec<u8>, AssetError> {
        Err(AssetError::Io(format!("{name}: not available")))
    };
    FbxImporter.import(ImportInput {
        path: Path::new("damaged.fbx"),
        bytes,
        settings: &serde_json::json!({}),
        read_relative: &read_relative,
        sub_ids: &mut sub_ids,
    })
}

fn uuid_v5() -> concordia_core::id::Uuid {
    concordia_core::id::Uuid::new_v5(&concordia_core::id::Uuid::NAMESPACE_OID, b"fbx-robustness")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn truncated_files_never_panic(source in 0usize..3, cut in 0.0f64..1.0) {
        let bytes = &SOURCES[source];
        let _ = import(&bytes[..(bytes.len() as f64 * cut) as usize]);
    }

    #[test]
    fn corrupted_bytes_never_panic(source in 0usize..3, edits in prop::collection::vec((0.0f64..1.0, any::<u8>()), 1..16)) {
        let mut bytes = SOURCES[source].clone();
        for (at, value) in edits {
            let i = ((bytes.len() - 1) as f64 * at) as usize;
            bytes[i] = value;
        }
        let _ = import(&bytes);
    }
}
