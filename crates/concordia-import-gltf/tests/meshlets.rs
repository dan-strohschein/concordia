//! glTF imports with meshlets (M4 4.2): the setting, the DDC and the sample corpus.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use common::Project;
use concordia_assets::{
    AssetError, DerivedCache, ImportInput, Importer, MeshData, SubIds, decode, import_jobs,
};
use concordia_cluster::MeshletMesh;
use concordia_core::ddc::DdcKey;
use concordia_core::id::{AssetId, TypedId};
use concordia_import_gltf::GltfImporter;

const MANIFEST: &str = include_str!("../../../tests/fixtures/gltf-sample-assets/manifest.ron");

/// Sets the sidecar's `meshlets` and re-imports.
fn set_meshlets(
    p: &mut Project,
    rel: &Path,
    value: &str,
) -> Result<concordia_assets::ImportReport, AssetError> {
    let sidecar = p.dir.path().join(format!("{}.import.ron", rel.display()));
    let text = std::fs::read_to_string(&sidecar).unwrap();
    let start = text
        .find("\"meshlets\": ")
        .expect("new sidecars declare meshlets");
    let end = start + text[start..].find(',').unwrap();
    let text = format!("{}\"meshlets\": {value}{}", &text[..start], &text[end..]);
    std::fs::write(&sidecar, text).unwrap();
    p.db.import(rel)
}

/// Every mesh sub-asset of an import, decoded.
fn meshes(p: &mut Project, report: &concordia_assets::ImportReport) -> Vec<(String, MeshData)> {
    report
        .sub_assets
        .keys()
        .filter(|k| k.starts_with("mesh/"))
        .map(|k| (k.clone(), p.db.load_mesh(&Project::sub(report, k)).unwrap()))
        .collect()
}

fn check(name: &str, mesh: &MeshData) {
    let m = MeshletMesh::decode(&mesh.meshlets).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(
        m.source_triangles as usize,
        mesh.indices.len() / 3,
        "{name}"
    );
    assert_eq!(m.source_vertices as usize, mesh.positions.len(), "{name}");
    assert!(!m.meshlets.is_empty(), "{name}");
}

#[test]
fn ddc_reuses_meshlets_when_source_unchanged() {
    let mut p = Project::new();
    p.sample("Box", "glTF-Binary", "Box.glb");
    let rel = Path::new("Box/glTF-Binary/Box.glb");
    let first = set_meshlets(&mut p, rel, "true").unwrap();
    assert!(!first.cached);
    let built = meshes(&mut p, &first);
    check("Box", &built[0].1);
    let second = p.db.import(rel).unwrap();
    assert!(second.cached, "the second import is a cache hit");
    assert_eq!(meshes(&mut p, &second), built);
}

/// An in-memory cache counting puts.
#[derive(Default)]
struct Counting {
    blobs: Mutex<BTreeMap<DdcKey, Vec<u8>>>,
    puts: Mutex<usize>,
}

impl DerivedCache for Counting {
    fn get(&self, key: &DdcKey) -> Result<Option<Vec<u8>>, AssetError> {
        Ok(self.blobs.lock().unwrap().get(key).cloned())
    }
    fn put(&self, key: &DdcKey, bytes: &[u8]) -> Result<(), AssetError> {
        *self.puts.lock().unwrap() += 1;
        self.blobs.lock().unwrap().insert(*key, bytes.to_vec());
        Ok(())
    }
}

#[test]
fn meshlets_survive_an_unrelated_setting_change() {
    let bytes = std::fs::read(common::samples().join("Box/glTF-Binary/Box.glb")).unwrap();
    let cache = Counting::default();
    let import = |settings: serde_json::Value| {
        let parent = AssetId::try_from_uuid(concordia_core::id::Uuid::new_v5(
            &concordia_core::id::Uuid::NAMESPACE_OID,
            b"gltf-meshlets",
        ))
        .unwrap();
        let mut sub_ids = SubIds::new(parent, BTreeMap::new());
        let read = |n: &str| -> Result<Vec<u8>, AssetError> { Err(AssetError::Io(n.into())) };
        let out = GltfImporter
            .import(ImportInput {
                path: Path::new("Box.glb"),
                bytes: &bytes,
                settings: &settings,
                read_relative: &read,
                sub_ids: &mut sub_ids,
                derived: &cache,
                jobs: import_jobs().unwrap(),
            })
            .unwrap();
        decode::<MeshData>(&out.artifacts["mesh/Mesh/0"].bytes)
            .unwrap()
            .meshlets
    };
    let a = import(serde_json::json!({ "meshlets": true }));
    let b = import(serde_json::json!({ "meshlets": true, "emissive_reference_ev100": 3.0 }));
    assert_eq!(a, b);
    assert_eq!(*cache.puts.lock().unwrap(), 1, "built once, then reused");
}

/// The sample models the corpus tests cover (one test each, so they run in parallel).
const CORPUS: [(&str, &str, &str); 11] = [
    (
        "AlphaBlendModeTest",
        "glTF-Binary",
        "AlphaBlendModeTest.glb",
    ),
    ("BoomBox", "glTF-Binary", "BoomBox.glb"),
    ("Box", "glTF-Binary", "Box.glb"),
    ("FlightHelmet", "glTF", "FlightHelmet.gltf"),
    ("Fox", "glTF-Binary", "Fox.glb"),
    ("MetalRoughSpheres", "glTF-Binary", "MetalRoughSpheres.glb"),
    (
        "MetalRoughSpheresNoTextures",
        "glTF-Binary",
        "MetalRoughSpheresNoTextures.glb",
    ),
    (
        "NormalTangentMirrorTest",
        "glTF-Binary",
        "NormalTangentMirrorTest.glb",
    ),
    ("NormalTangentTest", "glTF-Binary", "NormalTangentTest.glb"),
    ("OrientationTest", "glTF-Binary", "OrientationTest.glb"),
    ("SimpleSkin", "glTF", "SimpleSkin.gltf"),
];

#[test]
fn the_corpus_tests_cover_every_sample_model() {
    let mut manifest: Vec<String> = MANIFEST
        .lines()
        .filter_map(|l| l.trim().strip_prefix("\"Models/"))
        .map(|p| p[..p.find('/').unwrap()].to_owned())
        .collect();
    manifest.dedup();
    let covered: Vec<String> = CORPUS.iter().map(|c| c.0.to_owned()).collect();
    assert_eq!(covered, manifest);
}

/// The model imports with `meshlets: true` (every mesh gets valid meshlets), or, if skinned,
/// refuses it and gets none on its skinned meshes under `auto`. Returns the meshes `auto` built.
fn corpus_model(index: usize) -> Vec<String> {
    let (model, variant, file) = CORPUS[index];
    let mut p = Project::new();
    let auto = p.sample(model, variant, file);
    let mut auto_built = Vec::new();
    for (name, mesh) in meshes(&mut p, &auto) {
        if !mesh.meshlets.is_empty() {
            check(&name, &mesh);
            auto_built.push(name);
        }
    }
    let rel = Path::new(model).join(variant).join(file);
    match set_meshlets(&mut p, &rel, "true") {
        Ok(report) => {
            for (name, mesh) in meshes(&mut p, &report) {
                check(&format!("{model}/{name}"), &mesh);
            }
        }
        Err(e) => {
            assert!(e.to_string().contains("skinned"), "{model}: {e}");
            for (name, mesh) in meshes(&mut p, &auto) {
                let skinned = !mesh.joints.is_empty();
                assert!(!(skinned && !mesh.meshlets.is_empty()), "{model}/{name}");
            }
        }
    }
    auto_built
}

macro_rules! corpus_tests {
    ($($name:ident = $index:expr, auto builds $auto:expr;)*) => {$(
        #[test]
        fn $name() {
            let built = corpus_model($index);
            assert_eq!(built.len(), $auto, "{}: auto built {built:?}", CORPUS[$index].0);
        }
    )*};
}

corpus_tests! {
    gltf_corpus_alpha_blend_mode_test = 0, auto builds 0;
    gltf_corpus_boom_box = 1, auto builds 0;
    gltf_corpus_box = 2, auto builds 0;
    gltf_corpus_flight_helmet = 3, auto builds 4;
    gltf_corpus_fox = 4, auto builds 0;
    gltf_corpus_metal_rough_spheres = 5, auto builds 5;
    gltf_corpus_metal_rough_spheres_no_textures = 6, auto builds 98;
    gltf_corpus_normal_tangent_mirror_test = 7, auto builds 0;
    gltf_corpus_normal_tangent_test = 8, auto builds 0;
    gltf_corpus_orientation_test = 9, auto builds 0;
    gltf_corpus_simple_skin = 10, auto builds 0;
}
