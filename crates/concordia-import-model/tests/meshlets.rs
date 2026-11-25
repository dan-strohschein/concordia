//! The model importers' `meshlets` setting and its build cache (M4 4.2 Q3, Q4).

use std::collections::BTreeMap;
use std::sync::Mutex;

use concordia_assets::{AssetError, DerivedCache, MeshData, import_jobs};
use concordia_cluster::MeshletMesh;
use concordia_core::ddc::DdcKey;
use concordia_import_model::{
    AUTO_MIN_TRIANGLES, MeshletContext, MeshletMode, ModelSettings, attach_meshlets,
};

/// An in-memory cache counting hits and puts.
#[derive(Default)]
struct Counting {
    blobs: Mutex<BTreeMap<DdcKey, Vec<u8>>>,
    hits: Mutex<usize>,
    puts: Mutex<usize>,
}

impl DerivedCache for Counting {
    fn get(&self, key: &DdcKey) -> Result<Option<Vec<u8>>, AssetError> {
        let found = self.blobs.lock().unwrap().get(key).cloned();
        *self.hits.lock().unwrap() += usize::from(found.is_some());
        Ok(found)
    }
    fn put(&self, key: &DdcKey, bytes: &[u8]) -> Result<(), AssetError> {
        *self.puts.lock().unwrap() += 1;
        self.blobs.lock().unwrap().insert(*key, bytes.to_vec());
        Ok(())
    }
}

/// A strip of `triangles` triangles (a 2×N grid), optionally skinned.
fn strip(triangles: usize, skinned: bool) -> MeshData {
    let quads = triangles.div_ceil(2);
    let mut positions = Vec::new();
    for i in 0..=quads {
        positions.push([i as f32, 0.0, 0.0]);
        positions.push([i as f32, 1.0, 0.0]);
    }
    let mut indices = Vec::new();
    for q in 0..quads as u32 {
        let a = 2 * q;
        indices.extend([a, a + 2, a + 3, a, a + 3, a + 1]);
    }
    indices.truncate(3 * triangles);
    let n = positions.len();
    MeshData {
        normals: vec![[0.0, 0.0, 1.0]; n],
        joints: if skinned { vec![[0; 4]; n] } else { vec![] },
        weights: if skinned {
            vec![[1.0, 0.0, 0.0, 0.0]; n]
        } else {
            vec![]
        },
        positions,
        indices,
        ..MeshData::default()
    }
}

fn attach(parts: &mut [MeshData], mode: MeshletMode, cache: &Counting) -> Result<(), String> {
    let ctx = MeshletContext {
        mode,
        jobs: import_jobs().unwrap(),
        cache,
    };
    attach_meshlets(parts, &ctx)
}

fn decoded(part: &MeshData) -> MeshletMesh {
    MeshletMesh::decode(&part.meshlets).unwrap()
}

#[test]
fn auto_builds_only_static_meshes_over_8k_triangles() {
    assert_eq!(AUTO_MIN_TRIANGLES, 8192);
    let cache = Counting::default();
    // Two material parts of one mesh: 4,200 + 4,200 > 8,192 together.
    let mut rock = [strip(4200, false), strip(4200, false)];
    attach(&mut rock, MeshletMode::Auto, &cache).unwrap();
    for part in &rock {
        assert_eq!(decoded(part).source_triangles, 4200);
    }
    let mut exactly = [strip(8192, false)];
    attach(&mut exactly, MeshletMode::Auto, &cache).unwrap();
    assert!(
        exactly[0].meshlets.is_empty(),
        "8,192 is not more than 8,192"
    );
    let mut skinned = [strip(10_000, true)];
    attach(&mut skinned, MeshletMode::Auto, &cache).unwrap();
    assert!(skinned[0].meshlets.is_empty(), "skinned meshes are skipped");
}

#[test]
fn meshlets_true_and_false() {
    let cache = Counting::default();
    let mut small = [strip(10, false)];
    attach(&mut small, MeshletMode::Always, &cache).unwrap();
    assert_eq!(decoded(&small[0]).source_triangles, 10);
    let mut big = [strip(9000, false)];
    attach(&mut big, MeshletMode::Never, &cache).unwrap();
    assert!(big[0].meshlets.is_empty());
}

#[test]
fn meshlets_true_on_a_skinned_mesh_is_an_error() {
    let cache = Counting::default();
    let mut skinned = [strip(10, false), strip(10, true)];
    let err = attach(&mut skinned, MeshletMode::Always, &cache).unwrap_err();
    assert!(err.contains("skinned"), "{err}");
}

#[test]
fn the_setting_reads_auto_true_and_false() {
    let read =
        |v: serde_json::Value| ModelSettings::from_json(&serde_json::json!({ "meshlets": v }));
    assert_eq!(read("auto".into()).unwrap().meshlets, MeshletMode::Auto);
    assert_eq!(read(true.into()).unwrap().meshlets, MeshletMode::Always);
    assert_eq!(read(false.into()).unwrap().meshlets, MeshletMode::Never);
    assert_eq!(
        ModelSettings::from_json(&serde_json::json!({}))
            .unwrap()
            .meshlets,
        MeshletMode::Auto
    );
    assert_eq!(
        ModelSettings::defaults()["meshlets"],
        "auto",
        "written into new sidecars"
    );
}

#[test]
fn unknown_meshlets_value_is_an_error() {
    for bad in [
        serde_json::json!("sometimes"),
        serde_json::json!(3),
        serde_json::json!(null),
    ] {
        let err = ModelSettings::from_json(&serde_json::json!({ "meshlets": bad })).unwrap_err();
        assert!(err.contains("meshlets"), "{err}");
    }
}

#[test]
fn the_cache_is_reused_and_ignores_unread_streams() {
    let cache = Counting::default();
    let mut first = [strip(300, false)];
    attach(&mut first, MeshletMode::Always, &cache).unwrap();
    let mut second = [strip(300, false)];
    second[0].normals = vec![[0.0, 1.0, 0.0]; second[0].positions.len()];
    second[0].uv0 = vec![[0.5, 0.5]; second[0].positions.len()];
    attach(&mut second, MeshletMode::Always, &cache).unwrap();
    assert_eq!(
        (*cache.puts.lock().unwrap(), *cache.hits.lock().unwrap()),
        (1, 1)
    );
    assert_eq!(first[0].meshlets, second[0].meshlets);
}

#[test]
fn a_corrupt_cache_entry_is_rebuilt() {
    let cache = Counting::default();
    let mut part = [strip(300, false)];
    attach(&mut part, MeshletMode::Always, &cache).unwrap();
    let good = part[0].meshlets.clone();
    for blob in cache.blobs.lock().unwrap().values_mut() {
        blob.truncate(10);
    }
    let mut again = [strip(300, false)];
    attach(&mut again, MeshletMode::Always, &cache).unwrap();
    assert_eq!(again[0].meshlets, good);
    assert_eq!(*cache.puts.lock().unwrap(), 2, "rebuilt and re-cached");
}
